[<img alt="github" src="https://img.shields.io/badge/github-rttfd/bondybird-37a8e0?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/rttfd/bondybird)
[<img alt="crates.io" src="https://img.shields.io/crates/v/bondybird.svg?style=for-the-badge&color=ff8b94&logo=rust" height="20">](https://crates.io/crates/bondybird)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-bondybird-bedc9c?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/bondybird)

![Dall-E generated bondybird image](https://raw.githubusercontent.com/rttfd/static/refs/heads/main/bondybird/bondybird.png)

# `BondyBird`

`BondyBird` is a BR/EDR (Classic) Bluetooth Host implementation for embedded systems, built on top of Embassy and `bt-hci` crates and designed for `no_std` environments. It provides a **parallel task architecture** that separates HCI event processing from API command handling for optimal performance and responsiveness.

## Key Features

- 🚀 **Parallel Processing**: Separate tasks for HCI events and API requests
- 🔒 **Thread-Safe**: Embassy mutex-based shared state management  
- ⚡ **Low Latency**: Immediate HCI event processing without blocking
- 📦 **Memory Efficient**: `no_std` compatible with heapless collections
- 🛡️ **Type Safe**: Strong typing with newtype wrappers for addresses
- 🔧 **Easy Integration**: Simple API with async/await support

## Architecture

`BondyBird` uses a **parallel task architecture** that optimizes for performance and responsiveness:

1. **HCI Event Processor Task** - Dedicated task for processing incoming HCI events
2. **API Request Processor Task** - Handles API requests and executes HCI commands
3. **Shared State** - Thread-safe shared Bluetooth state using Embassy mutexes
4. **Static Channels** - Embassy channels for communication between API functions and tasks

```text
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────────┐
│  API Functions  │    │  API REQUEST     │    │   API Request       │
│                 │───▶│    CHANNEL       │───▶│   Processor Task    │
│                 │    └──────────────────┘    │                     │
└─────────────────┘                            └─────────────────────┘
        ▲                                               │
        │              ┌──────────────────┐             │
        └──────────────│  API RESPONSE    │◀────────────┘
                       │    CHANNEL       │
                       └──────────────────┘
                                                                       
                       ┌──────────────────┐    ┌─────────────────────┐
                       │   BondyBird      │    │   HCI Event         │
                       │  Shared State    │◀───│   Processor Task    │
                       │   (Mutex)        │    │                     │
                       │                  │    └─────────────────────┘
                       └──────────────────┘             ▲
                                ▲                       │
                                │                       │
                                │              ┌──────────────────┐
                                └──────────────│  HCI Controller  │
                                               │   (Transport)    │
                                               └──────────────────┘
```

## Benefits

- **🔄 Parallel Processing**: HCI events and API requests are processed independently
- **⏱️ Low Latency**: Events are processed immediately without blocking API operations  
- **🧵 Thread Safety**: Shared state is properly synchronized with Embassy mutexes
- **📈 Scalability**: Easy to add more processing tasks if needed
- **🎯 Type Safety**: Strong typing prevents common errors at compile time

## Quickstart

```rust,no_run,ignore
use bondybird::{hci_event_processor, api_request_processor, api, init_bluetooth_host, BluetoothHostOptions};
use bt_hci::controller::ExternalController;
use embassy_executor::Spawner;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Initialize BluetoothHost with custom options (or use default)
    let options = BluetoothHostOptions::default(); // Uses GIAC, default inquiry duration
    init_bluetooth_host(options).await.expect("Failed to initialize BluetoothHost");
    
    // Create your transport and controller  
    let transport = YourTransport::new();
    let controller = ExternalController::new(transport);
    let controller_ref = Box::leak(Box::new(controller));
    
    // Spawn both tasks for parallel processing
    spawner.spawn(hci_event_processor(controller_ref)).unwrap();
    spawner.spawn(api_request_processor(controller_ref)).unwrap();
    
    // Use API functions from anywhere in your application
    let _ = api::start_discovery().await;
    let devices = api::get_devices().await.unwrap();
    let _ = api::connect_device("AA:BB:CC:DD:EE:FF").await;
}
```

## API Usage

The API is designed to be simple and intuitive. Before using any API functions, you must initialize the BluetoothHost with your desired configuration:

```rust,no_run,ignore
use bondybird::{api, init_bluetooth_host, BluetoothHostOptions, constants};

// Initialize with default options
init_bluetooth_host(BluetoothHostOptions::default()).await?;

// Or initialize with custom options
let options = BluetoothHostOptions {
    lap: constants::GIAC,           // General Inquiry Access Code
    inquiry_length: 10,             // 10 * 1.28s = 12.8 seconds
    num_responses: 20,              // Maximum 20 responses
};
init_bluetooth_host(options).await?;

// Start device discovery
api::start_discovery().await?;

// Get discovered devices
let devices = api::get_devices().await?;
for device in devices {
    println!("Found device: {:?}", device.addr.format_hex());
}

// Connect to a device
api::connect_device("AA:BB:CC:DD:EE:FF").await?;

// Check connection state
let state = api::get_state().await?;
println!("Bluetooth state: {:?}", state);

// Disconnect
api::disconnect_device("AA:BB:CC:DD:EE:FF").await?;
```

## Configuration

`BondyBird` supports runtime configuration of Bluetooth inquiry parameters through `BluetoothHostOptions`:

```rust,no_run,ignore
use bondybird::{BluetoothHostOptions, constants};

// Default configuration
let default_options = BluetoothHostOptions::default();

// Custom configuration
let custom_options = BluetoothHostOptions {
    lap: constants::GIAC,           // Local Area Protocol - use GIAC for general inquiry
    inquiry_length: 5,              // Inquiry duration: 5 * 1.28s = 6.4 seconds
    num_responses: 15,              // Maximum number of device responses
};

// Alternative LAP codes (if needed)
let limited_inquiry = BluetoothHostOptions {
    lap: [0x00, 0x8B, 0x9E],       // LIAC - Limited Inquiry Access Code
    inquiry_length: 8,
    num_responses: 10,
};
```

### Configuration Parameters

- **`lap`**: Local Area Protocol (3 bytes) - determines which devices respond to inquiry
  - `GIAC` ([0x33, 0x8B, 0x9E]): General Inquiry - discovers all discoverable devices
  - `LIAC` ([0x00, 0x8B, 0x9E]): Limited Inquiry - discovers devices in limited discoverable mode
- **`inquiry_length`**: Duration of device discovery in units of 1.28 seconds (1-48)
- **`num_responses`**: Maximum number of device responses to collect (0-255, 0 = unlimited)

## Integration with Applications

`BondyBird` is designed to be easily integrated with applications. The API functions are simple and use static channels for communication with the Bluetooth tasks. This makes it easy to integrate with web servers, REST APIs, or any other application architecture.

### REST API Integration Example

```rust,no_run,ignore
// In your application handler
async fn discover_devices() -> Result<(), BluetoothError> {
    bondybird::api::start_discovery().await
}

async fn list_devices() -> Result<Vec<BluetoothDevice>, BluetoothError> {
    bondybird::api::get_devices().await
}

async fn connect_to_device(address: &str) -> Result<(), BluetoothError> {
    bondybird::api::connect_device(address).await
}
```

## Example: Complete Bluetooth Flow

```rust,no_run,ignore
use bondybird::{api, init_bluetooth_host, BluetoothHostOptions};
use embassy_time::{Duration, Timer};

async fn bluetooth_example() -> Result<(), BluetoothError> {
    // 0. Initialize BluetoothHost first
    let options = BluetoothHostOptions {
        lap: [0x33, 0x8B, 0x9E],       // GIAC - General Inquiry Access Code
        inquiry_length: 8,              // 8 * 1.28s = ~10 seconds
        num_responses: 10,              // Maximum 10 device responses
    };
    init_bluetooth_host(options).await?;

    // 1. Start discovery
    api::start_discovery().await?;

    // 2. Wait for discovery to find devices  
    Timer::after(Duration::from_secs(5)).await;

    // 3. Get discovered devices
    let devices = api::get_devices().await?;
    println!("Found {} devices", devices.len());

    // 4. Connect to the first device if available
    if let Some(device) = devices.first() {
        let addr_str = device.addr.format_hex();
        api::connect_device(&addr_str).await?;
        println!("Connected to device: {}", addr_str);
    }

    // 5. Check Bluetooth state
    let state = api::get_state().await?;
    println!("Bluetooth state: {:?}", state);

    Ok(())
}
```

## Features

- 🚀 **Parallel Processing**: Separate tasks for HCI events and API requests prevent blocking
- 🔒 **Thread-Safe**: Embassy mutex-based shared state management ensures data consistency  
- ⚡ **Low Latency**: Immediate HCI event processing without blocking API operations
- 📦 **`no_std` Compatible**: Designed for embedded systems with minimal resources
- 🛡️ **Type Safe**: Strong typing with newtype wrappers prevents common errors
- 🔄 **Async-first Design**: Built on Embassy crates for efficient resource usage
- 📡 **Static Channels**: Efficient communication with zero heap allocations
- 🔧 **Direct HCI Integration**: Uses bt-hci for standardized controller communication
- ⚙️ **Runtime Configuration**: Configure inquiry parameters (GIAC, duration) at startup
- 🎯 **Client-Controlled Initialization**: Initialize BluetoothHost when your application is ready

## Supported Bluetooth Operations

- ✅ Device Discovery (Inquiry)
- ✅ Device Connection/Disconnection  
- ✅ RSSI Information
- ✅ Device Class parsing
- ✅ Remote Name Resolution
- ✅ Connection State Management

## License

The MIT License (MIT)
Copyright © 2025 rttf.dev

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the “Software”), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
