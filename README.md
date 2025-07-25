[<img alt="github" src="https://img.shields.io/badge/github-rttfd/bondybird-37a8e0?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/rttfd/bondybird)
[<img alt="crates.io" src="https://img.shields.io/crates/v/bondybird.svg?style=for-the-badge&color=ff8b94&logo=rust" height="20">](https://crates.io/crates/bondybird)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-bondybird-bedc9c?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/bondybird)

![Dall-E generated bondybird image](https://raw.githubusercontent.com/rttfd/static/refs/heads/main/bondybird/bondybird.png)

# `BondyBird`

`BondyBird` is a comprehensive BR/EDR (Classic) Bluetooth Host implementation for embedded systems, built on Embassy and bt-hci crates for `no_std` environments. It features a parallel task architecture for optimal performance and responsiveness.

## Key Features

- **Complete Bluetooth Stack**: ACL connections, L2CAP channels, SDP service discovery, and A2DP audio streaming
- **Service Discovery Protocol (SDP)**: Full server and client implementation with service advertisement and discovery
- **Parallel Processing**: HCI events and API requests handled in separate tasks for maximum throughput
- **Thread-Safe**: Embassy mutex-based shared state with lock-free operation paths
- **Low Latency**: Immediate HCI event processing with zero-copy data handling
- **Memory Efficient**: `heapless` collections, `no_std` compatible, zero dynamic allocation
- **Type Safe**: Strong typing for addresses, handles, and protocol identifiers
- **Async/Await API**: Modern Rust async patterns throughout

## Architecture

`BondyBird` uses a parallel Embassy task architecture:

- **HCI Event Processor**: Handles incoming HCI events and sends internal commands
- **API Request Processor**: Processes API requests and generates responses
- **Internal Command Processor**: Executes HCI commands triggered by events (fire-and-forget)
- **SDP Server**: Manages service records and handles service discovery requests
- **A2DP Profile**: Provides audio streaming capabilities with SBC codec support

## Implemented Protocols & Profiles

### Core Stack
- **HCI (Host Controller Interface)**: Complete command/event handling
- **ACL (Asynchronous Connectionless)**: Connection management and data routing
- **L2CAP (Logical Link Control and Adaptation Protocol)**: Channel management and signaling

### Service Discovery
- **SDP Server**: Service record management, service search, attribute retrieval
- **SDP Client**: Remote service discovery, attribute queries, result caching
- **Service Records**: Standard Bluetooth service classes with UUID conversion

### Audio Profile
- **A2DP (Advanced Audio Distribution Profile)**: Audio streaming over Bluetooth
- **AVDTP (Audio/Video Distribution Transport Protocol)**: Stream endpoint management
- **SBC Codec**: Sub-band coding for audio compression

## Quickstart

```rust,ignore
use bondybird::{processor, BluetoothHostOptions, init_bluetooth_host};
use bt_hci::controller::ExternalController;
use embassy_executor::Spawner;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Initialize Bluetooth host with configuration
    let options = BluetoothHostOptions::default();
    init_bluetooth_host(options).await.unwrap();

    // Set up HCI transport and controller
    let transport = YourTransport::new();
    let controller = ExternalController::new(transport);
    let controller_ref = Box::leak(Box::new(controller));
    
    // Spawn the Bluetooth processor task
    spawner.spawn(processor::run::<YourTransport, 4, 512>(controller_ref)).unwrap();
    
    // Use API functions for device discovery and connection
    bondybird::api::start_discovery().await.unwrap();
    let devices = bondybird::api::get_devices().await.unwrap();
    
    if let Some(device) = devices.first() {
        let addr_str = device.addr.format_hex();
        bondybird::api::connect_device(&addr_str).await.unwrap();
        println!("Connected to: {}", addr_str);
    }
}
```

## API Usage

The API provides high-level functions for common Bluetooth operations:

- **Device Discovery**: `start_discovery()`, `stop_discovery()`, `get_devices()`
- **Connection Management**: `connect_device()`, `disconnect_device()`, `get_paired_devices()`
- **State Management**: `get_state()`, `get_local_info()`
- **Device Information**: `get_device_name()`

## SDP Service Discovery

```rust,ignore
use bondybird::sdp::{SdpServer, SdpClient, ServiceRecord, ServiceClassId};

// Server side - advertise services
let mut sdp_server = SdpServer::new();
let mut audio_service = ServiceRecord::new(0x10001, ServiceClassId::AudioSource);
audio_service.set_service_name("My Audio Device").unwrap();
audio_service.add_l2cap_protocol(0x0019).unwrap(); // A2DP PSM
sdp_server.add_service_record(audio_service).unwrap();

// Client side - discover services
let mut sdp_client = SdpClient::new();
let audio_uuid = ServiceClassId::AudioSource.to_uuid();
let transaction_id = sdp_client.discover_services(
    remote_address, 
    &[audio_uuid], 
    10
).unwrap();
```

## Example: Complete Bluetooth Flow with SDP

```rust,ignore
use bondybird::{processor, BluetoothHostOptions, sdp::ServiceClassId};
use embassy_time::{Duration, Timer};

async fn bluetooth_example() -> Result<(), bondybird::BluetoothError> {
    // Configure discovery parameters
    let options = BluetoothHostOptions {
        lap: [0x33, 0x8B, 0x9E],  // GIAC - General Inquiry Access Code
        inquiry_length: 8,         // ~10 seconds discovery time  
        num_responses: 10,         // Stop after 10 devices found
    };
    
    // Initialize and start discovery
    bondybird::init_bluetooth_host(options).await.unwrap();
    bondybird::api::start_discovery().await?;
    
    // Wait for discovery to complete
    Timer::after(Duration::from_secs(10)).await;
    let devices = bondybird::api::get_devices().await?;
    println!("Found {} devices", devices.len());
    
    // Connect to first device and discover its services
    if let Some(device) = devices.first() {
        let addr_str = device.addr.format_hex();
        bondybird::api::connect_device(&addr_str).await?;
        println!("Connected to device: {}", addr_str);
        
        // Discover audio services
        let mut sdp_client = bondybird::sdp::SdpClient::new();
        let audio_uuid = ServiceClassId::AudioSource.to_uuid();
        let _transaction = sdp_client.discover_services(
            device.addr, 
            &[audio_uuid], 
            5
        )?;
        
        println!("Service discovery initiated for audio services");
    }
    
    let state = bondybird::api::get_state().await?;
    println!("Bluetooth state: {:?}", state);
    Ok(())
}
```

## License

MIT License © 2025 rttf.dev
