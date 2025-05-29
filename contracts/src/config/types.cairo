use starknet::contract_address::ContractAddress;

#[derive(Drop, Serde, starknet::Store)]
pub struct Market {
    pub question: ByteArray,
    pub creator: ContractAddress,
    pub start_time: u64,
    pub end_time: u64,
    pub is_resolved: bool,
    pub winning_outcome_id: u32,
}

