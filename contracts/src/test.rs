#![cfg(test)]

//! Unit tests for the Stakcast prediction hub.
//!
//! The tests use a minimal SEP-41 compatible mock token so that staking,
//! fee payouts and claims can be asserted against real balances.

use crate::{
    BetPlaced, EmergencyPaused, FeeRecipientUpdated, Initialized, MarketCreated, MarketResolved,
    MarketToggled, ModeratorUpdated, PlatformFeeUpdated, PredictionHub, PredictionHubClient,
    WinningsClaimed,
};
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, ContractEvents, Events as _, Ledger as _},
    vec, Address, Env, Event, String,
};

// ============================================================
//  Mock SEP-41 token
// ============================================================

#[contracttype]
pub enum TokenKey {
    Admin,
    Balance(Address),
    Allowance(AllowanceKey),
}

#[contracttype]
#[derive(Clone)]
pub struct AllowanceKey {
    pub from: Address,
    pub spender: Address,
}

#[contract]
pub struct TestToken;

#[contractimpl]
impl TestToken {
    pub fn init(env: Env, admin: Address) {
        env.storage().instance().set(&TokenKey::Admin, &admin);
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let admin: Address = env.storage().instance().get(&TokenKey::Admin).unwrap();
        admin.require_auth();
        let balance = Self::balance(env.clone(), to.clone());
        env.storage()
            .persistent()
            .set(&TokenKey::Balance(to), &(balance + amount));
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&TokenKey::Balance(id))
            .unwrap_or(0)
    }

    pub fn approve(env: Env, from: Address, spender: Address, amount: i128, _expiry: u32) {
        from.require_auth();
        env.storage().persistent().set(
            &TokenKey::Allowance(AllowanceKey { from, spender }),
            &amount,
        );
    }

    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&TokenKey::Allowance(AllowanceKey { from, spender }))
            .unwrap_or(0)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        Self::move_balance(&env, &from, &to, amount);
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        let key = AllowanceKey {
            from: from.clone(),
            spender,
        };
        let allowance: i128 = env
            .storage()
            .persistent()
            .get(&TokenKey::Allowance(key.clone()))
            .unwrap_or(0);
        assert!(allowance >= amount, "insufficient allowance");
        env.storage()
            .persistent()
            .set(&TokenKey::Allowance(key), &(allowance - amount));
        Self::move_balance(&env, &from, &to, amount);
    }

    pub fn name(env: Env) -> String {
        String::from_str(&env, "Stakcast Test Token")
    }

    pub fn symbol(env: Env) -> String {
        String::from_str(&env, "STT")
    }

    pub fn decimals(_env: Env) -> u32 {
        7
    }

    fn move_balance(env: &Env, from: &Address, to: &Address, amount: i128) {
        let from_balance: i128 = env
            .storage()
            .persistent()
            .get(&TokenKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(from_balance >= amount, "insufficient balance");
        env.storage()
            .persistent()
            .set(&TokenKey::Balance(from.clone()), &(from_balance - amount));
        let to_balance: i128 = env
            .storage()
            .persistent()
            .get(&TokenKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&TokenKey::Balance(to.clone()), &(to_balance + amount));
    }
}

// ============================================================
//  Test helpers
// ============================================================

/// Deploys the mock token and the prediction hub, then initialises the hub.
fn setup() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let token_id = env.register(TestToken, ());
    let token_admin = Address::generate(&env);
    TestTokenClient::new(&env, &token_id).init(&token_admin);

    let contract_id = env.register(PredictionHub, ());
    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    PredictionHubClient::new(&env, &contract_id).initialize(
        &admin,
        &fee_recipient,
        &token_id,
        &500,
        &0,
        &0,
        &0,
        &0,
        &0,
    );

    (env, contract_id, token_id, admin, fee_recipient)
}

/// Creates a two-choice market ending one hour after the current ledger time
/// and returns its id.
fn create_market(env: &Env, contract_id: &Address, creator: &Address) -> u32 {
    let end_time = env.ledger().timestamp() + 3_600;
    PredictionHubClient::new(env, contract_id).create_predictions(
        creator,
        &String::from_str(env, "Will BTC close above 100k?"),
        &String::from_str(env, "Resolves on the 1st of next month."),
        &vec![
            &env,
            String::from_str(env, "Yes"),
            String::from_str(env, "No"),
        ],
        &String::from_str(env, "crypto"),
        &end_time,
        &1,
        &String::from_str(env, "https://example.com/btc.png"),
    )
}

/// Mints `amount` to `user` and approves the hub to pull it as a stake.
fn fund_and_approve(token: &TestTokenClient, contract_id: &Address, user: &Address, amount: i128) {
    token.mint(user, &amount);
    token.approve(user, contract_id, &amount, &u32::MAX);
}

// ============================================================
//  Tests
// ============================================================

#[test]
fn test_initialize_sets_state() {
    let (env, contract_id, token_id, admin, fee_recipient) = setup();
    let client = PredictionHubClient::new(&env, &contract_id);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_fee_recipient(), fee_recipient);
    assert_eq!(client.get_protocol_token(), token_id);
    assert_eq!(client.get_platform_fee(), 500);
    assert_eq!(client.get_prediction_count(), 0);
    assert_eq!(client.get_total_value_locked(), 0);
    assert_eq!(client.get_moderator_count(), 0);
    assert!(!client.is_paused());
}

#[test]
fn test_initialize_twice_panics() {
    let (env, contract_id, token_id, admin, fee_recipient) = setup();
    let client = PredictionHubClient::new(&env, &contract_id);

    let result = client.try_initialize(&admin, &fee_recipient, &token_id, &500, &0, &0, &0, &0, &0);
    assert!(result.is_err());
}

#[test]
fn test_create_market_requires_moderator() {
    let (env, contract_id, _token_id, _admin, _fee_recipient) = setup();
    let client = PredictionHubClient::new(&env, &contract_id);
    let stranger = Address::generate(&env);

    let result = client.try_create_predictions(
        &stranger,
        &String::from_str(&env, "Unauthorised"),
        &String::from_str(&env, "nope"),
        &vec![&env, String::from_str(&env, "Yes")],
        &String::from_str(&env, "general"),
        &(env.ledger().timestamp() + 3_600),
        &0,
        &String::from_str(&env, ""),
    );
    assert!(result.is_err());
}

#[test]
fn test_moderator_can_create_market() {
    let (env, contract_id, _token_id, admin, _fee_recipient) = setup();
    let client = PredictionHubClient::new(&env, &contract_id);
    let moderator = Address::generate(&env);

    client.add_moderator(&admin, &moderator);
    assert!(client.is_moderator(&moderator));
    assert_eq!(client.get_moderator_count(), 1);

    let market_id = create_market(&env, &contract_id, &moderator);
    assert_eq!(market_id, 1);
    assert_eq!(client.get_prediction_count(), 1);

    let market = client.get_prediction(&market_id);
    assert_eq!(market.market_id, 1);
    assert_eq!(market.choices.len(), 2);
    assert!(market.is_open);
    assert!(!market.is_resolved);
    assert_eq!(market.market_type, 1);
    assert_eq!(market.total_pool, 0);
    assert_eq!(client.get_all_crypto_predictions().len(), 1);
    assert_eq!(client.get_all_open_markets().len(), 1);

    client.remove_moderator(&admin, &moderator);
    assert!(!client.is_moderator(&moderator));
    assert_eq!(client.get_moderator_count(), 0);
}

#[test]
fn test_betting_pulls_stake_and_records_activity() {
    let (env, contract_id, token_id, admin, _fee_recipient) = setup();
    let client = PredictionHubClient::new(&env, &contract_id);
    let token = TestTokenClient::new(&env, &token_id);
    let user = Address::generate(&env);

    let market_id = create_market(&env, &contract_id, &admin);
    fund_and_approve(&token, &contract_id, &user, 1_000);
    client.buy_shares(&user, &market_id, &0, &600);

    // The stake moved from the user to the contract.
    assert_eq!(token.balance(&user), 400);
    assert_eq!(token.balance(&contract_id), 600);

    let market = client.get_prediction(&market_id);
    assert_eq!(market.total_pool, 600);
    assert_eq!(market.choices.get_unchecked(0).staked_amount, 600);
    assert_eq!(client.get_market_liquidity(&market_id), 600);
    assert_eq!(client.get_total_value_locked(), 600);
    assert_eq!(client.get_all_bets_for_user(&user).len(), 1);
    assert_eq!(client.get_bet_count_for_market(&user, &market_id), 1);

    let bet = client.get_choice_and_bet(&user, &market_id, &0);
    assert_eq!(bet.choice, 0);
    assert_eq!(bet.amount, 600);
    assert!(!bet.claimed);
    assert_eq!(client.get_market_activity(&market_id).len(), 1);
    assert!(client.is_market_open_for_betting(&market_id));
}
// ============================================================
//  Event tests
// ============================================================
//
// Every event is defined with `#[contractevent]`, so it is published with its
// own name as the first topic (for example `market_created`) and can be
// compared against the XDR emitted by the contract.
//
// `Env::events` only exposes the events of the most recent invocation, so
// `last_events` is called immediately after the call under test.

fn last_events(env: &Env, contract_id: &Address) -> ContractEvents {
    env.events().all().filter_by_contract(contract_id)
}

#[test]
fn test_initialize_emits_event() {
    let (env, contract_id, token_id, admin, _fee_recipient) = setup();

    assert_eq!(
        last_events(&env, &contract_id),
        [Initialized {
            admin: admin.clone(),
            token: token_id.clone(),
        }
        .to_xdr(&env, &contract_id)],
    );
}

#[test]
fn test_create_market_emits_event() {
    let (env, contract_id, _token_id, admin, _fee_recipient) = setup();
    let client = PredictionHubClient::new(&env, &contract_id);
    let end_time = env.ledger().timestamp() + 3_600;

    let market_id = create_market(&env, &contract_id, &admin);

    assert_eq!(
        last_events(&env, &contract_id),
        [MarketCreated {
            market_id,
            creator: admin.clone(),
            title: String::from_str(&env, "Will BTC close above 100k?"),
            end_time,
            market_type: 1,
        }
        .to_xdr(&env, &contract_id)],
    );
    assert_eq!(client.get_prediction_count(), 1);
}

#[test]
fn test_bet_and_claim_emit_events() {
    let (env, contract_id, token_id, admin, _fee_recipient) = setup();
    let client = PredictionHubClient::new(&env, &contract_id);
    let token = TestTokenClient::new(&env, &token_id);
    let user = Address::generate(&env);

    let market_id = create_market(&env, &contract_id, &admin);

    fund_and_approve(&token, &contract_id, &user, 1_000);
    client.buy_shares(&user, &market_id, &0, &600);
    assert_eq!(
        last_events(&env, &contract_id),
        [BetPlaced {
            market_id,
            user: user.clone(),
            choice: 0,
            amount: 600,
        }
        .to_xdr(&env, &contract_id)],
    );

    client.resolve_prediction(&admin, &market_id, &0);
    assert_eq!(
        last_events(&env, &contract_id),
        [MarketResolved {
            market_id,
            resolver: admin.clone(),
            winning_choice: 0,
            payout_pool: 570,
        }
        .to_xdr(&env, &contract_id)],
    );

    // 600 staked on the winning choice, minus the 5% (500 bps) protocol fee.
    assert_eq!(client.get_payout_pool(&market_id), 570);
    let payout = client.claim(&user, &market_id);
    assert_eq!(payout, 570);
    assert_eq!(
        last_events(&env, &contract_id),
        [WinningsClaimed {
            market_id,
            user: user.clone(),
            amount: 570,
        }
        .to_xdr(&env, &contract_id)],
    );
}

#[test]
fn test_admin_actions_emit_events() {
    let (env, contract_id, _token_id, admin, _fee_recipient) = setup();
    let client = PredictionHubClient::new(&env, &contract_id);
    let moderator = Address::generate(&env);
    let new_recipient = Address::generate(&env);
    let market_id = create_market(&env, &contract_id, &admin);

    client.add_moderator(&admin, &moderator);
    assert_eq!(
        last_events(&env, &contract_id),
        [ModeratorUpdated {
            moderator: moderator.clone(),
            enabled: true,
        }
        .to_xdr(&env, &contract_id)],
    );

    client.remove_moderator(&admin, &moderator);
    assert_eq!(
        last_events(&env, &contract_id),
        [ModeratorUpdated {
            moderator: moderator.clone(),
            enabled: false,
        }
        .to_xdr(&env, &contract_id)],
    );

    client.set_platform_fee(&admin, &250);
    assert_eq!(
        last_events(&env, &contract_id),
        [PlatformFeeUpdated { platform_fee: 250 }.to_xdr(&env, &contract_id)],
    );

    client.set_fee_recipient(&admin, &new_recipient);
    assert_eq!(
        last_events(&env, &contract_id),
        [FeeRecipientUpdated {
            recipient: new_recipient.clone(),
        }
        .to_xdr(&env, &contract_id)],
    );

    client.toggle_market_status(&admin, &market_id);
    assert_eq!(
        last_events(&env, &contract_id),
        [MarketToggled {
            market_id,
            is_open: false,
        }
        .to_xdr(&env, &contract_id)],
    );

    client.emergency_pause(&admin);
    assert_eq!(
        last_events(&env, &contract_id),
        [EmergencyPaused { paused: true }.to_xdr(&env, &contract_id)],
    );

    client.emergency_unpause(&admin);
    assert_eq!(
        last_events(&env, &contract_id),
        [EmergencyPaused { paused: false }.to_xdr(&env, &contract_id)],
    );

    // The resulting state matches the events that were emitted.
    assert!(!client.is_moderator(&moderator));
    assert_eq!(client.get_platform_fee(), 250);
    assert_eq!(client.get_fee_recipient(), new_recipient);
    assert!(!client.is_paused());
    assert!(!client.is_market_open_for_betting(&market_id));
}

#[test]
fn test_resolve_without_winners_refunds_and_charges_no_fee() {
    let (env, contract_id, token_id, admin, fee_recipient) = setup();
    let client = PredictionHubClient::new(&env, &contract_id);
    let token = TestTokenClient::new(&env, &token_id);
    let user = Address::generate(&env);

    let market_id = create_market(&env, &contract_id, &admin);
    fund_and_approve(&token, &contract_id, &user, 500);
    client.buy_shares(&user, &market_id, &1, &500);

    // Nobody backed choice 0, so no winnings are paid out and no fee is taken.
    client.resolve_prediction(&admin, &market_id, &0);
    assert_eq!(client.get_payout_pool(&market_id), 500);

    // The lone backer of choice 1 is refunded in full.
    let refund = client.collect_winnings(&user, &market_id);
    assert_eq!(refund, 500);
    assert_eq!(token.balance(&user), 500);
    assert_eq!(token.balance(&fee_recipient), 0);
    assert_eq!(client.get_total_value_locked(), 0);
}
