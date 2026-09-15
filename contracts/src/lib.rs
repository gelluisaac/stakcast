#![no_std]

//! Stakcast Prediction Hub - Soroban (Stellar) smart contract.
//!
//! Decentralised prediction markets on Stellar. The protocol token is a
//! SEP-41 token (the native XLM Stellar Asset Contract or any other Stellar
//! Asset Contract token). Flows:
//!
//!   1. `initialize` wires up the admin, fee recipient, protocol token and
//!      protocol limits.
//!   2. Moderators / the admin create markets with `create_predictions`.
//!   3. Users `buy_shares` (alias `place_bet`) with the protocol token. The
//!      contract pulls the stake with `transfer_from`, so the caller must have
//!      approved the contract on the token first.
//!   4. A moderator / the admin resolves a market with `resolve_prediction`.
//!   5. Winners `claim` (alias `collect_winnings`) their share of the payout
//!      pool, proportional to their stake on the winning choice.

use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, log, token, vec, Address, BytesN, Env,
    IntoVal, String, TryFromVal, Val, Vec,
};

/// Fees are expressed in basis points (1 / 10_000).
const FEE_DENOMINATOR: i128 = 10_000;
/// Upper bound on the number of choices supported per market.
const MAX_CHOICES: u32 = 32;
/// Storage TTL management (in ledgers).
const TTL_THRESHOLD: u32 = 100;
const TTL_EXTEND: u32 = 5_000_000;

// ============================================================
//  Storage keys
// ============================================================

#[contracttype]
pub enum DataKey {
    // Access control
    Admin,
    Moderator(Address),
    ModeratorCount,
    // Market data
    PredictionCount,
    Prediction(u32),
    UserBets(UserMarketKey),
    UserMarketIds(Address),
    MarketAnalytics(u32),
    // Pool / fee accounting
    MarketLiquidity(u32),
    PayoutPool(u32),
    TotalValueLocked,
    // Token & fees
    FeeRecipient,
    PlatformFee,
    ProtocolToken,
    // Safety switches
    IsPaused,
    MarketCreationPaused,
    BettingPaused,
    ResolutionPaused,
    // Limits
    MinMarketDuration,
    MaxMarketDuration,
    ResolutionWindow,
    MinBetAmount,
    MaxBetAmount,
}

/// Composite key identifying a user's bets within a single market.
#[contracttype]
#[derive(Clone)]
pub struct UserMarketKey {
    pub user: Address,
    pub market_id: u32,
}

// ============================================================
//  Types
// ============================================================

#[contracttype]
#[derive(Clone)]
pub struct Choice {
    pub label: String,
    pub staked_amount: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct Market {
    pub market_id: u32,
    pub creator: Address,
    pub title: String,
    pub description: String,
    pub category: String,
    pub choices: Vec<Choice>,
    pub end_time: u64,
    pub is_open: bool,
    pub is_resolved: bool,
    pub winning_choice: Option<u32>,
    pub total_pool: i128,
    /// 0 = general, 1 = crypto, 2 = sports
    pub market_type: u32,
    pub image_url: String,
}

#[contracttype]
#[derive(Clone)]
pub struct UserBet {
    pub market_id: u32,
    pub choice: u32,
    pub choice_label: String,
    pub amount: i128,
    pub claimed: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct Activity {
    pub user: Address,
    pub choice: u32,
    pub amount: i128,
    pub timestamp: u64,
}

// ============================================================
//  Events
// ============================================================
//
// Every event carries its own name as the first topic (the snake_case form of
// the struct name), so off-chain consumers such as the Stakcast indexer can
// filter by `topic[0] = "<event_name>"` and by the indexed fields below.

/// Emitted once when the protocol is initialised.
#[contractevent]
#[derive(Clone)]
pub struct Initialized {
    #[topic]
    pub admin: Address,
    pub token: Address,
}

/// Emitted when a new market is created.
#[contractevent]
#[derive(Clone)]
pub struct MarketCreated {
    #[topic]
    pub market_id: u32,
    pub creator: Address,
    pub title: String,
    pub end_time: u64,
    pub market_type: u32,
}

/// Emitted when a user stakes on a market choice.
#[contractevent]
#[derive(Clone)]
pub struct BetPlaced {
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub choice: u32,
    pub amount: i128,
}

/// Emitted when a market is resolved by a moderator or the admin.
#[contractevent]
#[derive(Clone)]
pub struct MarketResolved {
    #[topic]
    pub market_id: u32,
    pub resolver: Address,
    pub winning_choice: u32,
    pub payout_pool: i128,
}

/// Emitted when a user claims winnings, or a refund.
#[contractevent]
#[derive(Clone)]
pub struct WinningsClaimed {
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub amount: i128,
}

/// Emitted when a moderator is added or removed.
#[contractevent]
#[derive(Clone)]
pub struct ModeratorUpdated {
    #[topic]
    pub moderator: Address,
    pub enabled: bool,
}

/// Emitted when a market is opened or closed by a moderator.
#[contractevent]
#[derive(Clone)]
pub struct MarketToggled {
    #[topic]
    pub market_id: u32,
    pub is_open: bool,
}

/// Emitted when the protocol is frozen or resumed.
#[contractevent]
#[derive(Clone)]
pub struct EmergencyPaused {
    #[topic]
    pub paused: bool,
}

/// Emitted when the fee recipient changes.
#[contractevent]
#[derive(Clone)]
pub struct FeeRecipientUpdated {
    #[topic]
    pub recipient: Address,
}

/// Emitted when the platform fee changes.
#[contractevent]
#[derive(Clone)]
pub struct PlatformFeeUpdated {
    pub platform_fee: i128,
}

// ============================================================
//  Storage helpers
// ============================================================

fn instance_get<T: TryFromVal<Env, Val>>(env: &Env, key: &DataKey) -> Option<T> {
    env.storage().instance().get(key)
}

fn instance_set<T: IntoVal<Env, Val>>(env: &Env, key: &DataKey, value: &T) {
    env.storage().instance().set(key, value);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
}

fn persistent_get<T: TryFromVal<Env, Val>>(env: &Env, key: &DataKey) -> Option<T> {
    env.storage().persistent().get(key)
}

fn persistent_set<T: IntoVal<Env, Val>>(env: &Env, key: &DataKey, value: &T) {
    env.storage().persistent().set(key, value);
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND);
}

fn instance_has(env: &Env, key: &DataKey) -> bool {
    env.storage().instance().has(key)
}

// ============================================================
//  Access control helpers
// ============================================================

fn assert_only_admin(env: &Env, who: &Address) {
    let admin: Address = instance_get(env, &DataKey::Admin).expect("not initialized");
    assert!(&admin == who, "only admin");
}

fn is_moderator(env: &Env, who: &Address) -> bool {
    persistent_get::<bool>(env, &DataKey::Moderator(who.clone())).unwrap_or(false)
}

fn assert_moderator_or_admin(env: &Env, who: &Address) {
    let admin: Address = instance_get(env, &DataKey::Admin).expect("not initialized");
    assert!(
        &admin == who || is_moderator(env, who),
        "only moderator or admin"
    );
}

fn assert_not_paused(env: &Env) {
    assert!(
        !instance_get::<bool>(env, &DataKey::IsPaused).unwrap_or(false),
        "contract paused"
    );
}

fn protocol_token(env: &Env) -> Address {
    instance_get(env, &DataKey::ProtocolToken).expect("not initialized")
}

// ============================================================
//  Contract
// ============================================================

#[contract]
pub struct PredictionHub;

#[contractimpl]
impl PredictionHub {
    // ------------------------------------------------------------
    //  Initialization
    // ------------------------------------------------------------

    /// Initializes the protocol. Must be called exactly once, right after
    /// deployment. The `admin` must authorise the call.
    #[allow(clippy::too_many_arguments)]
    pub fn initialize(
        env: Env,
        admin: Address,
        fee_recipient: Address,
        token: Address,
        platform_fee: i128,
        min_market_duration: u64,
        max_market_duration: u64,
        resolution_window: u64,
        min_bet_amount: i128,
        max_bet_amount: i128,
    ) {
        assert!(!instance_has(&env, &DataKey::Admin), "already initialized");
        assert!((0..FEE_DENOMINATOR).contains(&platform_fee), "invalid fee");
        assert!(min_bet_amount >= 0, "invalid min bet");
        assert!(max_bet_amount >= min_bet_amount, "invalid max bet");
        admin.require_auth();

        instance_set(&env, &DataKey::Admin, &admin);
        instance_set(&env, &DataKey::FeeRecipient, &fee_recipient);
        instance_set(&env, &DataKey::ProtocolToken, &token);
        instance_set(&env, &DataKey::PlatformFee, &platform_fee);
        instance_set(&env, &DataKey::MinMarketDuration, &min_market_duration);
        instance_set(&env, &DataKey::MaxMarketDuration, &max_market_duration);
        instance_set(&env, &DataKey::ResolutionWindow, &resolution_window);
        instance_set(&env, &DataKey::MinBetAmount, &min_bet_amount);
        instance_set(&env, &DataKey::MaxBetAmount, &max_bet_amount);
        instance_set(&env, &DataKey::PredictionCount, &0u32);
        instance_set(&env, &DataKey::TotalValueLocked, &0i128);

        Initialized {
            admin: admin.clone(),
            token: token.clone(),
        }
        .publish(&env);
    }

    // ------------------------------------------------------------
    //  Market creation
    // ------------------------------------------------------------

    /// Creates a new prediction market and returns its id. Only moderators and
    /// the admin may create markets.
    #[allow(clippy::too_many_arguments)]
    pub fn create_predictions(
        env: Env,
        creator: Address,
        title: String,
        description: String,
        choices: Vec<String>,
        category: String,
        end_time: u64,
        market_type: u32,
        image_url: String,
    ) -> u32 {
        assert_not_paused(&env);
        assert!(
            !instance_get::<bool>(&env, &DataKey::MarketCreationPaused).unwrap_or(false),
            "market creation paused"
        );
        assert_moderator_or_admin(&env, &creator);
        creator.require_auth();

        assert!(!title.is_empty(), "title required");
        assert!(
            !choices.is_empty() && choices.len() <= MAX_CHOICES,
            "invalid choice count"
        );

        let now = env.ledger().timestamp();
        assert!(end_time > now, "end time must be in the future");
        let min_duration: u64 = instance_get(&env, &DataKey::MinMarketDuration).unwrap_or(0);
        let max_duration: u64 = instance_get(&env, &DataKey::MaxMarketDuration).unwrap_or(0);
        let duration = end_time - now;
        if min_duration > 0 {
            assert!(duration >= min_duration, "market duration too short");
        }
        if max_duration > 0 {
            assert!(duration <= max_duration, "market duration too long");
        }

        let mut count: u32 = instance_get(&env, &DataKey::PredictionCount).unwrap_or(0);
        count += 1;

        let mut market_choices: Vec<Choice> = vec![&env];
        for choice in choices.iter() {
            market_choices.push_back(Choice {
                label: choice,
                staked_amount: 0,
            });
        }

        let market = Market {
            market_id: count,
            creator: creator.clone(),
            title: title.clone(),
            description,
            category,
            choices: market_choices,
            end_time,
            is_open: true,
            is_resolved: false,
            winning_choice: None,
            total_pool: 0,
            market_type,
            image_url,
        };

        persistent_set(&env, &DataKey::Prediction(count), &market);
        let empty_activity: Vec<Activity> = vec![&env];
        persistent_set(&env, &DataKey::MarketAnalytics(count), &empty_activity);
        instance_set(&env, &DataKey::PredictionCount, &count);

        MarketCreated {
            market_id: count,
            creator: creator.clone(),
            title,
            end_time,
            market_type,
        }
        .publish(&env);
        log!(&env, "market created");
        count
    }

    // ------------------------------------------------------------
    //  Betting
    // ------------------------------------------------------------

    /// Buys shares (places a bet) on `choice` of `market_id` for `amount`.
    ///
    /// The caller must have approved this contract on the protocol token for at
    /// least `amount` before calling, because the stake is pulled with
    /// `transfer_from`.
    pub fn buy_shares(env: Env, user: Address, market_id: u32, choice: u32, amount: i128) {
        assert_not_paused(&env);
        assert!(
            !instance_get::<bool>(&env, &DataKey::BettingPaused).unwrap_or(false),
            "betting paused"
        );
        assert!(amount > 0, "amount must be positive");

        let min_bet: i128 = instance_get(&env, &DataKey::MinBetAmount).unwrap_or(0);
        let max_bet: i128 = instance_get(&env, &DataKey::MaxBetAmount).unwrap_or(i128::MAX);
        if min_bet > 0 {
            assert!(amount >= min_bet, "amount below minimum");
        }
        if max_bet > 0 {
            assert!(amount <= max_bet, "amount above maximum");
        }

        let mut market: Market =
            persistent_get(&env, &DataKey::Prediction(market_id)).expect("market not found");
        assert!(market.is_open, "market closed");
        assert!(!market.is_resolved, "market resolved");
        assert!(env.ledger().timestamp() < market.end_time, "market ended");
        assert!(choice < market.choices.len(), "invalid choice");

        user.require_auth();

        let contract = env.current_contract_address();
        let token_id = protocol_token(&env);
        let client = token::TokenClient::new(&env, &token_id);
        client.transfer_from(&contract, &user, &contract, &amount);

        // Move the stake into the chosen choice and the overall pool.
        let mut chosen: Choice = market.choices.get_unchecked(choice);
        chosen.staked_amount += amount;
        market.choices.set(choice, chosen);
        market.total_pool += amount;

        // Record the bet for the user.
        let bets_key = DataKey::UserBets(UserMarketKey {
            user: user.clone(),
            market_id,
        });
        let mut bets: Vec<UserBet> = persistent_get(&env, &bets_key).unwrap_or(vec![&env]);
        let first_bet = bets.is_empty();
        bets.push_back(UserBet {
            market_id,
            choice,
            choice_label: market.choices.get_unchecked(choice).label.clone(),
            amount,
            claimed: false,
        });
        persistent_set(&env, &bets_key, &bets);

        if first_bet {
            let mut ids: Vec<u32> =
                persistent_get(&env, &DataKey::UserMarketIds(user.clone())).unwrap_or(vec![&env]);
            ids.push_back(market_id);
            persistent_set(&env, &DataKey::UserMarketIds(user.clone()), &ids);
        }

        // Analytics trail for the market.
        let mut activity: Vec<Activity> =
            persistent_get(&env, &DataKey::MarketAnalytics(market_id)).unwrap_or(vec![&env]);
        activity.push_back(Activity {
            user: user.clone(),
            choice,
            amount,
            timestamp: env.ledger().timestamp(),
        });
        persistent_set(&env, &DataKey::MarketAnalytics(market_id), &activity);

        // Accounting.
        let liquidity: i128 =
            persistent_get(&env, &DataKey::MarketLiquidity(market_id)).unwrap_or(0);
        persistent_set(
            &env,
            &DataKey::MarketLiquidity(market_id),
            &(liquidity + amount),
        );
        let tvl: i128 = instance_get(&env, &DataKey::TotalValueLocked).unwrap_or(0);
        instance_set(&env, &DataKey::TotalValueLocked, &(tvl + amount));

        persistent_set(&env, &DataKey::Prediction(market_id), &market);

        BetPlaced {
            market_id,
            user: user.clone(),
            choice,
            amount,
        }
        .publish(&env);
    }

    /// Alias for [`Self::buy_shares`] kept for API parity.
    pub fn place_bet(env: Env, user: Address, market_id: u32, choice: u32, amount: i128) {
        Self::buy_shares(env, user, market_id, choice, amount);
    }

    // ------------------------------------------------------------
    //  Resolution & payouts
    // ------------------------------------------------------------

    /// Resolves a market by declaring the winning choice. Only moderators and
    /// the admin may resolve markets.
    ///
    /// The protocol fee is taken from the total pool (only when there is at
    /// least one winning stake) and paid to the fee recipient immediately. The
    /// remaining payout pool stays in the contract until winners claim.
    pub fn resolve_prediction(env: Env, resolver: Address, market_id: u32, winning_choice: u32) {
        assert_not_paused(&env);
        assert!(
            !instance_get::<bool>(&env, &DataKey::ResolutionPaused).unwrap_or(false),
            "resolution paused"
        );
        assert_moderator_or_admin(&env, &resolver);
        resolver.require_auth();

        let mut market: Market =
            persistent_get(&env, &DataKey::Prediction(market_id)).expect("market not found");
        assert!(!market.is_resolved, "market already resolved");
        assert!(winning_choice < market.choices.len(), "invalid choice");

        let total_pool = market.total_pool;
        let winners_total = market.choices.get_unchecked(winning_choice).staked_amount;

        // The protocol fee is only charged when at least one winning stake
        // exists, otherwise every stake is refunded in full.
        let platform_fee: i128 = instance_get(&env, &DataKey::PlatformFee).unwrap_or(0);
        let mut fee: i128 = 0;
        if winners_total > 0 && platform_fee > 0 && total_pool > 0 {
            fee = total_pool * platform_fee / FEE_DENOMINATOR;
            if fee > total_pool {
                fee = total_pool;
            }
        }
        let payout_pool = total_pool - fee;

        market.is_resolved = true;
        market.is_open = false;
        market.winning_choice = Some(winning_choice);
        persistent_set(&env, &DataKey::Prediction(market_id), &market);
        persistent_set(&env, &DataKey::PayoutPool(market_id), &payout_pool);
        persistent_set(&env, &DataKey::MarketLiquidity(market_id), &0i128);

        let tvl: i128 = instance_get(&env, &DataKey::TotalValueLocked).unwrap_or(0);
        instance_set(
            &env,
            &DataKey::TotalValueLocked,
            &tvl.saturating_sub(total_pool),
        );

        if fee > 0 {
            let recipient: Address =
                instance_get(&env, &DataKey::FeeRecipient).expect("not initialized");
            let client = token::TokenClient::new(&env, &protocol_token(&env));
            client.transfer(&env.current_contract_address(), &recipient, &fee);
        }

        MarketResolved {
            market_id,
            resolver,
            winning_choice,
            payout_pool,
        }
        .publish(&env);
    }

    /// Claims the caller's winnings for a resolved market.
    ///
    /// Winning stakes receive a share of the payout pool proportional to their
    /// stake. When nobody backed the winning choice every stake is refunded in
    /// full, minus nothing (no fee is charged in that case).
    pub fn claim(env: Env, user: Address, market_id: u32) -> i128 {
        let market: Market =
            persistent_get(&env, &DataKey::Prediction(market_id)).expect("market not found");
        assert!(market.is_resolved, "market not resolved");
        let winning_choice = market.winning_choice.expect("no winning choice");

        user.require_auth();

        let bets_key = DataKey::UserBets(UserMarketKey {
            user: user.clone(),
            market_id,
        });
        let bets: Vec<UserBet> = persistent_get(&env, &bets_key).expect("no bets");
        assert!(!bets.is_empty(), "no bets");

        let winners_total = market.choices.get_unchecked(winning_choice).staked_amount;
        let payout_pool: i128 = persistent_get(&env, &DataKey::PayoutPool(market_id)).unwrap_or(0);

        let mut total_payout: i128 = 0;
        let mut updated: Vec<UserBet> = vec![&env];
        for bet in bets.iter() {
            let mut bet = bet;
            if !bet.claimed {
                let payout = if winners_total == 0 {
                    // No winner: refund the stake.
                    bet.amount
                } else if bet.choice == winning_choice {
                    bet.amount * payout_pool / winners_total
                } else {
                    0
                };
                if payout > 0 {
                    total_payout += payout;
                    bet.claimed = true;
                }
            }
            updated.push_back(bet);
        }

        persistent_set(&env, &bets_key, &updated);

        if total_payout > 0 {
            let client = token::TokenClient::new(&env, &protocol_token(&env));
            client.transfer(&env.current_contract_address(), &user, &total_payout);
        }

        WinningsClaimed {
            market_id,
            user,
            amount: total_payout,
        }
        .publish(&env);
        total_payout
    }

    /// Alias for [`Self::claim`] kept for API parity.
    pub fn collect_winnings(env: Env, user: Address, market_id: u32) -> i128 {
        Self::claim(env, user, market_id)
    }

    // ------------------------------------------------------------
    //  Queries
    // ------------------------------------------------------------

    pub fn get_prediction_count(env: Env) -> u32 {
        instance_get(&env, &DataKey::PredictionCount).unwrap_or(0)
    }

    pub fn get_prediction(env: Env, market_id: u32) -> Market {
        persistent_get(&env, &DataKey::Prediction(market_id)).expect("market not found")
    }

    pub fn get_all_predictions(env: Env) -> Vec<Market> {
        Self::markets_matching(&env, |_| true)
    }

    pub fn get_all_general_predictions(env: Env) -> Vec<Market> {
        Self::markets_matching(&env, |m| m.market_type == 0)
    }

    pub fn get_all_crypto_predictions(env: Env) -> Vec<Market> {
        Self::markets_matching(&env, |m| m.market_type == 1)
    }

    pub fn get_all_sports_predictions(env: Env) -> Vec<Market> {
        Self::markets_matching(&env, |m| m.market_type == 2)
    }

    pub fn get_all_open_markets(env: Env) -> Vec<Market> {
        let now = env.ledger().timestamp();
        Self::markets_matching(&env, |m| m.is_open && !m.is_resolved && m.end_time > now)
    }

    /// Markets whose betting window closed but that are not resolved yet.
    pub fn get_all_locked_markets(env: Env) -> Vec<Market> {
        let now = env.ledger().timestamp();
        Self::markets_matching(&env, |m| !m.is_resolved && m.end_time <= now)
    }

    pub fn get_all_resolved_markets(env: Env) -> Vec<Market> {
        Self::markets_matching(&env, |m| m.is_resolved)
    }

    /// All markets the user has placed at least one bet on.
    pub fn get_all_bets_for_user(env: Env, user: Address) -> Vec<Market> {
        Self::user_markets_matching(&env, &user, |_| true)
    }

    /// Alias for [`Self::get_all_bets_for_user`].
    pub fn get_user_predictions(env: Env, user: Address) -> Vec<Market> {
        Self::get_all_bets_for_user(env, user)
    }

    pub fn get_user_crypto_predictions(env: Env, user: Address) -> Vec<Market> {
        Self::user_markets_matching(&env, &user, |m| m.market_type == 1)
    }

    pub fn get_user_sports_predictions(env: Env, user: Address) -> Vec<Market> {
        Self::user_markets_matching(&env, &user, |m| m.market_type == 2)
    }

    pub fn get_user_general_predictions(env: Env, user: Address) -> Vec<Market> {
        Self::user_markets_matching(&env, &user, |m| m.market_type == 0)
    }

    /// Total amount the user can still claim across every resolved market.
    pub fn get_user_claimable_amount(env: Env, user: Address) -> i128 {
        let ids: Vec<u32> =
            persistent_get(&env, &DataKey::UserMarketIds(user.clone())).unwrap_or(vec![&env]);
        let mut total: i128 = 0;
        for market_id in ids.iter() {
            let market: Market = match persistent_get(&env, &DataKey::Prediction(market_id)) {
                Some(market) => market,
                None => continue,
            };
            if !market.is_resolved {
                continue;
            }
            let winning_choice = match market.winning_choice {
                Some(choice) => choice,
                None => continue,
            };
            let winners_total = market.choices.get_unchecked(winning_choice).staked_amount;
            let payout_pool: i128 =
                persistent_get(&env, &DataKey::PayoutPool(market_id)).unwrap_or(0);
            let key = DataKey::UserBets(UserMarketKey {
                user: user.clone(),
                market_id,
            });
            let bets: Vec<UserBet> = persistent_get(&env, &key).unwrap_or(vec![&env]);
            for bet in bets.iter() {
                if bet.claimed {
                    continue;
                }
                if winners_total == 0 {
                    total += bet.amount;
                } else if bet.choice == winning_choice {
                    total += bet.amount * payout_pool / winners_total;
                }
            }
        }
        total
    }

    pub fn get_user_bets(env: Env, user: Address, market_id: u32) -> Vec<UserBet> {
        persistent_get(&env, &DataKey::UserBets(UserMarketKey { user, market_id }))
            .unwrap_or(vec![&env])
    }

    pub fn get_bet_count_for_market(env: Env, user: Address, market_id: u32) -> u32 {
        Self::get_user_bets(env, user, market_id).len()
    }

    pub fn get_choice_and_bet(env: Env, user: Address, market_id: u32, bet_index: u32) -> UserBet {
        Self::get_user_bets(env, user, market_id)
            .get(bet_index)
            .expect("bet not found")
    }

    pub fn get_market_activity(env: Env, market_id: u32) -> Vec<Activity> {
        persistent_get(&env, &DataKey::MarketAnalytics(market_id)).unwrap_or(vec![&env])
    }

    pub fn get_market_status(env: Env, market_id: u32) -> (bool, bool) {
        let market: Market =
            persistent_get(&env, &DataKey::Prediction(market_id)).expect("market not found");
        (market.is_open, market.is_resolved)
    }

    /// Amount currently staked on an unresolved market.
    pub fn get_market_liquidity(env: Env, market_id: u32) -> i128 {
        persistent_get(&env, &DataKey::MarketLiquidity(market_id)).unwrap_or(0)
    }

    pub fn get_payout_pool(env: Env, market_id: u32) -> i128 {
        persistent_get(&env, &DataKey::PayoutPool(market_id)).unwrap_or(0)
    }

    pub fn get_total_value_locked(env: Env) -> i128 {
        instance_get(&env, &DataKey::TotalValueLocked).unwrap_or(0)
    }

    pub fn get_betting_restrictions(env: Env) -> (i128, i128) {
        (
            instance_get(&env, &DataKey::MinBetAmount).unwrap_or(0),
            instance_get(&env, &DataKey::MaxBetAmount).unwrap_or(0),
        )
    }

    /// Returns the SEP-41 token used for staking.
    pub fn get_protocol_token(env: Env) -> Address {
        protocol_token(&env)
    }

    pub fn get_admin(env: Env) -> Address {
        instance_get(&env, &DataKey::Admin).expect("not initialized")
    }

    pub fn get_fee_recipient(env: Env) -> Address {
        instance_get(&env, &DataKey::FeeRecipient).expect("not initialized")
    }

    pub fn get_platform_fee(env: Env) -> i128 {
        instance_get(&env, &DataKey::PlatformFee).unwrap_or(0)
    }

    pub fn get_moderator_count(env: Env) -> u32 {
        instance_get(&env, &DataKey::ModeratorCount).unwrap_or(0)
    }

    pub fn is_moderator(env: Env, who: Address) -> bool {
        is_moderator(&env, &who)
    }

    pub fn is_paused(env: Env) -> bool {
        instance_get(&env, &DataKey::IsPaused).unwrap_or(false)
    }

    pub fn is_market_open_for_betting(env: Env, market_id: u32) -> bool {
        if instance_get::<bool>(&env, &DataKey::IsPaused).unwrap_or(false)
            || instance_get::<bool>(&env, &DataKey::BettingPaused).unwrap_or(false)
        {
            return false;
        }
        match persistent_get::<Market>(&env, &DataKey::Prediction(market_id)) {
            Some(market) => {
                market.is_open && !market.is_resolved && env.ledger().timestamp() < market.end_time
            }
            None => false,
        }
    }

    // ------------------------------------------------------------
    //  Internal query helpers
    // ------------------------------------------------------------

    fn markets_matching(env: &Env, predicate: impl Fn(&Market) -> bool) -> Vec<Market> {
        let count: u32 = instance_get(env, &DataKey::PredictionCount).unwrap_or(0);
        let mut out: Vec<Market> = vec![env];
        let mut id: u32 = 1;
        while id <= count {
            if let Some(market) = persistent_get::<Market>(env, &DataKey::Prediction(id)) {
                if predicate(&market) {
                    out.push_back(market);
                }
            }
            id += 1;
        }
        out
    }

    fn user_markets_matching(
        env: &Env,
        user: &Address,
        predicate: impl Fn(&Market) -> bool,
    ) -> Vec<Market> {
        let ids: Vec<u32> =
            persistent_get(env, &DataKey::UserMarketIds(user.clone())).unwrap_or(vec![env]);
        let mut out: Vec<Market> = vec![env];
        for market_id in ids.iter() {
            if let Some(market) = persistent_get::<Market>(env, &DataKey::Prediction(market_id)) {
                if predicate(&market) {
                    out.push_back(market);
                }
            }
        }
        out
    }

    // ------------------------------------------------------------
    //  Administration
    // ------------------------------------------------------------

    /// Grants moderator privileges to `moderator`. Admin only.
    pub fn add_moderator(env: Env, admin: Address, moderator: Address) {
        assert_only_admin(&env, &admin);
        admin.require_auth();
        if !is_moderator(&env, &moderator) {
            let count: u32 = instance_get(&env, &DataKey::ModeratorCount).unwrap_or(0);
            instance_set(&env, &DataKey::ModeratorCount, &(count + 1));
        }
        persistent_set(&env, &DataKey::Moderator(moderator.clone()), &true);
        ModeratorUpdated {
            moderator,
            enabled: true,
        }
        .publish(&env);
    }

    /// Revokes moderator privileges from `moderator`. Admin only.
    pub fn remove_moderator(env: Env, admin: Address, moderator: Address) {
        assert_only_admin(&env, &admin);
        admin.require_auth();
        if is_moderator(&env, &moderator) {
            let count: u32 = instance_get(&env, &DataKey::ModeratorCount).unwrap_or(0);
            instance_set(&env, &DataKey::ModeratorCount, &count.saturating_sub(1));
            env.storage()
                .persistent()
                .remove(&DataKey::Moderator(moderator.clone()));
        }
        ModeratorUpdated {
            moderator,
            enabled: false,
        }
        .publish(&env);
    }

    /// Updates the address that receives protocol fees. Admin only.
    pub fn set_fee_recipient(env: Env, admin: Address, recipient: Address) {
        assert_only_admin(&env, &admin);
        admin.require_auth();
        instance_set(&env, &DataKey::FeeRecipient, &recipient);
        FeeRecipientUpdated { recipient }.publish(&env);
    }

    /// Updates the platform fee in basis points. Admin only.
    pub fn set_platform_fee(env: Env, admin: Address, platform_fee: i128) {
        assert_only_admin(&env, &admin);
        admin.require_auth();
        assert!((0..FEE_DENOMINATOR).contains(&platform_fee), "invalid fee");
        instance_set(&env, &DataKey::PlatformFee, &platform_fee);
        PlatformFeeUpdated { platform_fee }.publish(&env);
    }

    /// Updates the minimum and maximum stake accepted per bet. Admin only.
    pub fn set_betting_limits(env: Env, admin: Address, min_bet: i128, max_bet: i128) {
        assert_only_admin(&env, &admin);
        admin.require_auth();
        assert!(min_bet >= 0, "invalid min bet");
        assert!(max_bet >= min_bet, "invalid max bet");
        instance_set(&env, &DataKey::MinBetAmount, &min_bet);
        instance_set(&env, &DataKey::MaxBetAmount, &max_bet);
    }

    /// Opens or closes an individual market. Moderator or admin only.
    pub fn toggle_market_status(env: Env, who: Address, market_id: u32) {
        assert_moderator_or_admin(&env, &who);
        who.require_auth();
        let mut market: Market =
            persistent_get(&env, &DataKey::Prediction(market_id)).expect("market not found");
        market.is_open = !market.is_open;
        persistent_set(&env, &DataKey::Prediction(market_id), &market);
        MarketToggled {
            market_id,
            is_open: market.is_open,
        }
        .publish(&env);
    }

    /// Freezes the whole protocol. Admin only.
    pub fn emergency_pause(env: Env, admin: Address) {
        assert_only_admin(&env, &admin);
        admin.require_auth();
        instance_set(&env, &DataKey::IsPaused, &true);
        EmergencyPaused { paused: true }.publish(&env);
    }

    /// Resumes the protocol. Admin only.
    pub fn emergency_unpause(env: Env, admin: Address) {
        assert_only_admin(&env, &admin);
        admin.require_auth();
        instance_set(&env, &DataKey::IsPaused, &false);
        EmergencyPaused { paused: false }.publish(&env);
    }

    /// Pauses / resumes market creation. Admin only.
    pub fn pause_market_creation(env: Env, admin: Address, paused: bool) {
        assert_only_admin(&env, &admin);
        admin.require_auth();
        instance_set(&env, &DataKey::MarketCreationPaused, &paused);
    }

    /// Pauses / resumes betting. Admin only.
    pub fn pause_betting(env: Env, admin: Address, paused: bool) {
        assert_only_admin(&env, &admin);
        admin.require_auth();
        instance_set(&env, &DataKey::BettingPaused, &paused);
    }

    /// Pauses / resumes market resolution. Admin only.
    pub fn pause_resolution(env: Env, admin: Address, paused: bool) {
        assert_only_admin(&env, &admin);
        admin.require_auth();
        instance_set(&env, &DataKey::ResolutionPaused, &paused);
    }

    /// Removes every market and resets the protocol counters. Admin only.
    /// Intended for testnets only: claims become impossible afterwards.
    pub fn remove_all_predictions(env: Env, admin: Address) {
        assert_only_admin(&env, &admin);
        admin.require_auth();
        let count: u32 = instance_get(&env, &DataKey::PredictionCount).unwrap_or(0);
        let mut id: u32 = 1;
        while id <= count {
            env.storage().persistent().remove(&DataKey::Prediction(id));
            env.storage()
                .persistent()
                .remove(&DataKey::MarketAnalytics(id));
            env.storage()
                .persistent()
                .remove(&DataKey::MarketLiquidity(id));
            env.storage().persistent().remove(&DataKey::PayoutPool(id));
            id += 1;
        }
        instance_set(&env, &DataKey::PredictionCount, &0u32);
        instance_set(&env, &DataKey::TotalValueLocked, &0i128);
    }

    /// Upgrades the contract wasm. Admin only.
    pub fn upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        assert_only_admin(&env, &admin);
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

#[cfg(test)]
mod test;
