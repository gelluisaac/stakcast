use starknet::ContractAddress;
use starknet::class_hash::ClassHash;

// ================ Market Types ================

/// Represents a general prediction market with binary (yes/no) outcomes
/// Used for any type of prediction that doesn't fit crypto or sports categories
#[derive(Drop, Serde, starknet::Store)]
pub struct PredictionMarket {
    pub title: ByteArray, // Market title/question
    pub market_id: u256, // Unique identifier for the market
    pub description: ByteArray, // Detailed description of the prediction
    pub choices: (Choice, Choice), // Binary choices (typically Yes/No)
    pub category: felt252, // Category identifier for market classification
    pub image_url: ByteArray, // URL to market image/icon
    pub is_resolved: bool, // Whether the market has been resolved
    pub is_open: bool, // Whether the market is accepting new bets
    pub end_time: u64, // Timestamp when the market closes
    pub winning_choice: Option<Choice>, // The winning choice after resolution
    pub total_pool: u256 // Total amount staked in the market
}

/// Represents a cryptocurrency price prediction market
/// Used for predictions about crypto asset prices (e.g., "Will BTC be above $X by date Y?")
#[derive(Drop, Serde, starknet::Store)]
pub struct CryptoPrediction {
    pub title: ByteArray,
    pub market_id: u256,
    pub description: ByteArray,
    pub choices: (Choice, Choice),
    pub category: felt252,
    pub image_url: ByteArray,
    pub is_resolved: bool,
    pub is_open: bool,
    pub end_time: u64,
    pub winning_choice: Option<Choice>,
    pub total_pool: u256,
    pub comparison_type: u8, // 0 -> less than amount, 1 -> greater than amount
    pub asset_key: felt252, // Identifier for the crypto asset (e.g., BTC, ETH)
    pub target_value: u128 // Target price value for the prediction
}

/// Represents a sports event prediction market
/// Used for predictions about sports match outcomes
#[derive(Drop, Serde, starknet::Store)]
pub struct SportsPrediction {
    pub title: ByteArray,
    pub market_id: u256,
    pub description: ByteArray,
    pub choices: (Choice, Choice),
    pub category: felt252,
    pub image_url: ByteArray,
    pub is_resolved: bool,
    pub is_open: bool,
    pub end_time: u64,
    pub winning_choice: Option<Choice>,
    pub total_pool: u256,
    pub event_id: u64, // External API event ID for automatic resolution
    pub team_flag: bool // Flag indicating if this is a team-based prediction
}

/// Represents a business prediction market with binary (yes/no) outcomes
/// Used for predictions about business (e.g., "Will over 50% of Fortune 500 companies adopt
/// blockchain solutions by 2026?")
#[derive(Drop, Serde, starknet::Store)]
pub struct BusinessPrediction {
    pub title: ByteArray, // Market title/question
    pub market_id: u256, // Unique identifier for the market
    pub description: ByteArray, // Detailed description of the prediction
    pub choices: (Choice, Choice), // Binary choices (typically Yes/No)
    pub category: felt252, // Category identifier for market classification
    pub image_url: ByteArray, // URL to market image/icon
    pub is_resolved: bool, // Whether the market has been resolved
    pub is_open: bool, // Whether the market is accepting new bets
    pub end_time: u64, // Timestamp when the market closes
    pub winning_choice: Option<Choice>, // The winning choice after resolution
    pub total_pool: u256, // Total amount staked in the market
    pub event_id: u64 // External API event ID for automatic resolution
}

// ================ Supporting Types ================

/// Represents a choice in a prediction market with its associated stake
#[derive(Copy, Serde, Drop, starknet::Store, PartialEq, Hash)]
pub struct Choice {
    pub label: felt252, // Text label for the choice
    pub staked_amount: u256 // Total amount staked on this choice
}

/// Represents a user's stake in a prediction market
#[derive(Drop, Serde, starknet::Store)]
pub struct UserStake {
    pub amount: u256, // Amount staked by the user
    pub claimed: bool // Whether the user has claimed their winnings
}

/// Represents a user's bet on a specific choice in a market
#[derive(Drop, Serde, starknet::Store)]
pub struct UserBet {
    pub choice: Choice, // The choice the user bet on
    pub stake: UserStake // The user's stake details
}

// ================ Contract Interface ================

/// Main interface for the Prediction Hub contract
/// Handles creation, management, and resolution of prediction markets
#[starknet::interface]
pub trait IPredictionHub<TContractState> {
    // ================ Market Creation ================

    /// Creates a new general prediction market with binary (yes/no) choices
    fn create_prediction(
        ref self: TContractState,
        title: ByteArray,
        description: ByteArray,
        choices: (felt252, felt252),
        category: felt252,
        image_url: ByteArray,
        end_time: u64,
    );

    /// Creates a cryptocurrency price prediction market
    /// Used for predictions about crypto asset prices
    fn create_crypto_prediction(
        ref self: TContractState,
        title: ByteArray,
        description: ByteArray,
        choices: (felt252, felt252),
        category: felt252,
        image_url: ByteArray,
        end_time: u64,
        comparison_type: u8,
        asset_key: felt252,
        target_value: u128,
    );

    /// Creates a sports event prediction market
    /// Used for predictions about sports match outcomes
    fn create_sports_prediction(
        ref self: TContractState,
        title: ByteArray,
        description: ByteArray,
        choices: (felt252, felt252),
        category: felt252,
        image_url: ByteArray,
        end_time: u64,
        event_id: u64,
        team_flag: bool,
    );

    /// Creates a new business prediction market with binary (yes/no) choicesAdd commentMore actions
    fn create_business_prediction(
        ref self: TContractState,
        title: ByteArray,
        description: ByteArray,
        choices: (felt252, felt252),
        category: felt252,
        image_url: ByteArray,
        end_time: u64,
        event_id: u64,
    );

    // ================ Market Queries ================

    /// Returns the total number of prediction markets created
    fn get_prediction_count(self: @TContractState) -> u256;

    /// Retrieves a specific prediction market by ID
    fn get_prediction(self: @TContractState, market_id: u256) -> PredictionMarket;

    /// Returns an array of all active general prediction markets
    fn get_all_predictions(self: @TContractState) -> Array<PredictionMarket>;

    /// Retrieves a specific crypto prediction by ID
    fn get_crypto_prediction(self: @TContractState, market_id: u256) -> CryptoPrediction;

    /// Returns an array of all active crypto prediction markets
    fn get_all_crypto_predictions(self: @TContractState) -> Array<CryptoPrediction>;

    /// Retrieves a specific sports prediction by ID
    fn get_sports_prediction(self: @TContractState, market_id: u256) -> SportsPrediction;

    /// Returns an array of all active sports prediction markets
    fn get_all_sports_predictions(self: @TContractState) -> Array<SportsPrediction>;

    /// Retrieves a specific business prediction market by IDAdd commentMore actions
    fn get_business_prediction(self: @TContractState, market_id: u256) -> BusinessPrediction;

    /// Returns an array of all active business prediction markets
    fn get_all_business_predictions(self: @TContractState) -> Array<BusinessPrediction>;

    // get current market status of markets 
    fn get_market_status(self: @TContractState, market_id: u256, market_type: u8) -> (bool, bool);

    // Returns the total number of bets placed on a market by all users.
    fn get_market_bet_count(self: @TContractState, market_id: u256, market_type: u8) -> u256;

    // ================ Betting Functions ================

    /// Places a bet on a specific market and choice
    /// Returns true if the bet was successfully placed
    fn place_bet(
        ref self: TContractState, market_id: u256, choice_idx: u8, amount: u256, market_type: u8,
    ) -> bool;


    fn place_wager(
        ref self: TContractState, market_id: u256, choice_idx: u8, amount: u256, market_type: u8,
    ) -> bool;

    /// Returns how many bets a user has placed on a specific market
    fn get_bet_count_for_market(
        self: @TContractState, user: ContractAddress, market_id: u256, market_type: u8,
    ) -> u8;

    /// Retrieves a specific bet made by a user
    fn get_choice_and_bet(
        self: @TContractState, user: ContractAddress, market_id: u256, market_type: u8, bet_idx: u8,
    ) -> UserBet;

    /// Returns the betting token contract address
    fn get_betting_token(self: @TContractState) -> ContractAddress;

    /// Returns total fees collected for a specific market
    fn get_market_fees(self: @TContractState, market_id: u256) -> u256;

    /// Returns total fees collected across all markets
    fn get_total_fees_collected(self: @TContractState) -> u256;

    /// Returns current betting restrictions (min, max amounts)
    fn get_betting_restrictions(self: @TContractState) -> (u256, u256);

    /// Returns market liquidity information
    fn get_market_liquidity(self: @TContractState, market_id: u256) -> u256;

    /// Returns total value locked across all markets
    fn get_total_value_locked(self: @TContractState) -> u256;

    /// Returns an array of all active general prediction markets
    fn get_active_prediction_markets(self: @TContractState) -> Array<PredictionMarket>;

    /// Returns an array of all active sport prediction markets
    fn get_active_sport_markets(self: @TContractState) -> Array<SportsPrediction>;

    /// Returns an array of all active crypto prediction markets
    fn get_active_crypto_markets(self: @TContractState) -> Array<CryptoPrediction>;

    /// Returns an array of all active business prediction markets
    fn get_active_business_markets(self: @TContractState) -> Array<BusinessPrediction>;

    /// Returns an array of all resolved general prediction markets
    fn get_resolved_prediction_markets(self: @TContractState) -> Array<PredictionMarket>;

    /// Returns an array of all resolved sport prediction markets
    fn get_resolved_sport_markets(self: @TContractState) -> Array<SportsPrediction>;

    /// Returns an array of all resolved crypto prediction markets
    fn get_resolved_crypto_markets(self: @TContractState) -> Array<CryptoPrediction>;

    /// Returns an array of all resolved business prediction markets
    fn get_resolved_business_markets(self: @TContractState) -> Array<BusinessPrediction>;

    fn is_prediction_market_open_for_betting(ref self: TContractState, market_id: u256) -> bool;

    fn is_crypto_market_open_for_betting(ref self: TContractState, market_id: u256) -> bool;

    fn is_sport_market_open_for_betting(ref self: TContractState, market_id: u256) -> bool;

    fn is_business_market_open_for_betting(ref self: TContractState, market_id: u256) -> bool;

    // ================ Market Resolution ================

    /// Resolves a general prediction market by setting the winning option
    fn resolve_prediction(ref self: TContractState, market_id: u256, winning_choice: u8);

    /// Manually resolves a crypto prediction market
    /// Override for the automatic resolution
    fn resolve_crypto_prediction_manually(
        ref self: TContractState, market_id: u256, winning_choice: u8,
    );

    /// Manually resolves a sports prediction market
    /// Override for the automatic resolution
    fn resolve_sports_prediction_manually(
        ref self: TContractState, market_id: u256, winning_choice: u8,
    );

    /// Automatically resolves a crypto prediction using oracle price data
    fn resolve_crypto_prediction(ref self: TContractState, market_id: u256);

    /// Resolves a sports prediction automatically based on event outcome
    fn resolve_sports_prediction(ref self: TContractState, market_id: u256, winning_choice: u8);

    /// Manually resolves a business prediction market
    /// Override for the automatic resolution
    fn resolve_business_prediction_manually(
        ref self: TContractState, market_id: u256, winning_choice: u8,
    );
    // ================ Winnings Management ================

    /// Allows a user to claim their winnings from a resolved prediction
    fn collect_winnings(ref self: TContractState, market_id: u256, market_type: u8, bet_idx: u8);

    /// Calculates total unclaimed winnings for a user across all markets
    fn get_user_claimable_amount(self: @TContractState, user: ContractAddress) -> u256;

    // ================ User Queries ================

    /// Returns all general prediction markets a specific user has participated in
    fn get_user_predictions(
        self: @TContractState, user: ContractAddress,
    ) -> Array<PredictionMarket>;

    /// Returns all crypto prediction markets a specific user has participated in
    fn get_user_crypto_predictions(
        self: @TContractState, user: ContractAddress,
    ) -> Array<CryptoPrediction>;

    /// Returns all sports prediction markets a specific user has participated in
    fn get_user_sports_predictions(
        self: @TContractState, user: ContractAddress,
    ) -> Array<SportsPrediction>;

    // ================ Administrative Functions ================

    /// Returns the contract admin address
    fn get_admin(self: @TContractState) -> ContractAddress;

    /// Returns the address receiving platform fees
    fn get_fee_recipient(self: @TContractState) -> ContractAddress;

    /// Sets a new fee recipient address
    fn set_fee_recipient(ref self: TContractState, recipient: ContractAddress);

    /// Opens or closes a market for new bets
    fn toggle_market_status(ref self: TContractState, market_id: u256, market_type: u8);

    /// Adds a new moderator who can create/resolve predictions
    fn add_moderator(ref self: TContractState, moderator: ContractAddress);

    /// Administrative function to reset all prediction markets
    fn remove_all_predictions(ref self: TContractState);

    // ================ Upgrade Functions ================

    /// Upgrades the contract implementation (admin only)
    fn upgrade(ref self: TContractState, impl_hash: ClassHash);

    // ================ Multi-Token Support Functions ================

    /// Places a bet using a specific token
    fn place_bet_with_token(
        ref self: TContractState,
        market_id: u256,
        choice_idx: u8,
        amount: u256,
        market_type: u8,
        token_name: felt252,
    ) -> bool;

    /// Places a wager using a specific token
    fn place_wager_with_token(
        ref self: TContractState,
        market_id: u256,
        choice_idx: u8,
        amount: u256,
        market_type: u8,
        token_name: felt252,
    ) -> bool;

    /// Gets the contract address for a supported token
    fn get_supported_token(self: @TContractState, token_name: felt252) -> ContractAddress;

    /// Gets the token used for a specific market
    fn get_market_token(self: @TContractState, market_id: u256) -> ContractAddress;

    /// Checks if a token is supported
    fn is_token_supported(self: @TContractState, token_name: felt252) -> bool;
}
