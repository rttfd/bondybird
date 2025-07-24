//! L2CAP (Logical Link Control and Adaptation Protocol) Implementation
//!
//! L2CAP provides connection-oriented and connectionless data services to upper layer
//! protocols with protocol multiplexing, segmentation and reassembly, and group abstractions.

pub mod packet;

// TODO: Uncomment when L2CAP is integrated with the rest of the system
// pub use packet::{
//     ChannelId, L2capError, L2capHeader, L2capPacket, ProtocolServiceMultiplexer,
//     cid, psm,
// };
