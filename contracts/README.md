# StakCast Smart Contracts

## Overview

StakCast is a prediction market platform built on StarkNet that allows users to create and participate in prediction markets using the SK token. The platform enables users to purchase units in real-world prediction markets and earn rewards based on correct predictions.

## Contract Architecture

### Core Contracts

1. **PredictionMarket.cairo**

   - Main contract for market operations
   - Handles market creation, unit purchases, and reward distribution
   - Uses SK token for all transactions
   - Implements access control for validators

2. **SKToken.cairo**
   - ERC20 token contract for platform transactions
   - Fixed rate: $1 = SK1000
   - Used for purchasing units and receiving rewards
   - Implements OpenZeppelin's ERC20 standard

### Supporting Contracts

1. **Interfaces/**

   - `IMarket.cairo`: Interface for prediction market functions
   - `IToken.cairo`: Interface for SK token functions

2. **Config/**
   - `types.cairo`: Market data structures
   - `errors.cairo`: Custom error definitions
   - `utils.cairo`: Utility functions

## Key Features

### Market Creation

- Create markets with multiple outcomes
- Set market duration and parameters
- No initial liquidity required
- Markets funded by participants

### Unit Purchase

- Purchase units using SK tokens
- Tokens held in escrow until market resolution
- Track user positions and outcome volumes

### Market Resolution

- Validators resolve market outcomes
- Decentralized validation planned for v2.0

### Reward Distribution

- Winners get original stake back
- Winners receive share of losing pool
- Proportional to unit ownership
- Automatic token distribution

## Technical Details

### Token Integration

- SK token used for all transactions
- `transfer_from` for purchases
- `transfer` for rewards
- No fees in version 0

### Access Control

- Admin role for validator management
- Validator role for market resolution
- OpenZeppelin's AccessControl implementation

### Storage Structure

- Markets: Map<market_id, Market>
- User positions: Map<(market_id, user, outcome_id), units>
- Outcome totals: Map<(market_id, outcome_id), total_units>
- Validators: Map<address, bool>

## Development Setup

### Prerequisites

- Starkdet development environment
- OpenZeppelin contracts

### Building

```bash
scarb build
```

### Testing

```bash
scarb test
```

### Deployment

1. Deploy SKToken contract
2. Deploy PredictionMarket contract with SKToken address
3. Set up initial validators

## License

MIT License
