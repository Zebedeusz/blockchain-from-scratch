# Full Client Blockchain Node - design

## Components

1. Transaction Pool
    - validation and prioritization of transactions.  
2. Storage
    - storage of current state and all the blocks.
3. Consensus Engine
    - validation and sealing of blocks.
4. State Machine
    - modyfing the state based on provided transitions.
5. Fork Choice
    - switching from one fork rules to another if necessary.

## Node Operation Flows

### Accepting extrinsics

API: submit_extrinsic(e: E) -> bool

Flow:

1. Extrinsic received by node
2. Node sends the extrinsic to transaction pool
3. Transaction pool validates and orders the extrinsic

### Accepting blocks

1. The node receives a block from the network to import
2. The node validates the block with consensus engine
    - Consensus engine runs all the transactions through the state machine to obtain new state
    - Consensus engine calculates hash of the new state
3. If valid, the node sets new state in storage to the state from the block and saves the new block

### Authoring blocks

1. The network sends a request to a given node to author next block
2. The node takes a given amount of extrinsics from the transaction pool
3. The node puts the transactions into a new block
4. The node asks storage for last block and current state
5. The node runs all the extrinsics through the state machine to obtain new state
6. The node asks consensus engine to validate and seal the block
7. The node stores the block in the storage and updates the state in the storage

## Tests
