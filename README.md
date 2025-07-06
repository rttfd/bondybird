[<img alt="github" src="https://img.shields.io/badge/github-rttfd/bondybird-37a8e0?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/rttfd/bondybird)
[<img alt="crates.io" src="https://img.shields.io/crates/v/bondybird.svg?style=for-the-badge&color=ff8b94&logo=rust" height="20">](https://crates.io/crates/bondybird)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-bondybird-bedc9c?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/bondybird)

![Dall-E generated bondybird image](https://raw.githubusercontent.com/rttfd/static/refs/heads/main/bondybird/bondybird.png)

# BondyBird

BondyBird is a BR/EDR (Classic) Bluetooth Host implementation for embedded systems, built on top of Embassy's async executor and designed for `no_std` environments. It provides a single-task, channel-based architecture that is both memory-efficient and easy to integrate with application code.

## Architecture

BondyBird uses a simplified architecture with:

1. **Single Manager Task** - A single async task that handles all Bluetooth operations
2. **Static Channels** - Embassy channels for communication between API functions and the manager
3. **Direct HCI Integration** - Using the bt-hci library for controller communication

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│  API Functions  │    │  API REQUEST     │    │                 │
│                 │───▶│    CHANNEL       │───▶│                 │
│                 │    └──────────────────┘    │   Bluetooth     │
└─────────────────┘                            │   Manager       │
        ▲                                      │    Task         │
        │                                      │                 │
        │              ┌──────────────────┐    │                 │
        └──────────────│  API RESPONSE    │◀───│                 │
                       │    CHANNEL       │    │                 │
                       └──────────────────┘    └─────────────────┘
                                                        │
                                                        │
                                                        ▼
                                               ┌──────────────────┐
                                               │  HCI Controller  │
                                               │  (select! loop)  │
                                               └──────────────────┘
```

## Quickstart

```rust,no_run,ignore
use bondybird::{bluetooth_manager_task, api};
use bt_hci::transport::YourTransport;
use embassy_executor::Spawner;

// Spawn the Bluetooth manager task (only one task needed)
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let transport = YourTransport::new();
    
    // Spawn the single Bluetooth manager task
    spawner.spawn(bluetooth_manager_task(transport)).unwrap();
    
    // Use API functions in your application
    let _ = api::start_discovery().await;
    let devices = api::get_devices().await.unwrap();
}
```

You can use the provided API functions in your application:
- `api::start_discovery()` - Start device discovery
- `api::get_devices()` - Get list of discovered devices
- `api::connect_device(address)` - Connect to a device
- `api::disconnect_device(address)` - Disconnect from a device
- `api::get_state()` - Get current Bluetooth state

## Integration with Applications

BondyBird is designed to be easily integrated with applications. The API functions are simple and use static channels for communication with the Bluetooth manager task. This makes it easy to integrate with web servers, REST APIs, or any other application architecture.

```rust,no_run,ignore
// In your application handler
async fn discover_devices() -> Result<(), Box<dyn std::error::Error>> {
    match bondybird::api::start_discovery().await {
        Ok(()) => println!("Discovery started"),
        Err(e) => println!("Error: {:?}", e),
    }
    Ok(())
}

async fn list_devices() -> Result<(), Box<dyn std::error::Error>> {
    match bondybird::api::get_devices().await {
        Ok(devices) => {
            for device in devices {
                println!("Device: {:?}", device);
            }
        },
        Err(e) => println!("Error: {:?}", e),
    }
    Ok(())
}
```

## Example: Complete Flow

```rust,no_run,ignore
// 1. Start discovery
let _ = bondybird::api::start_discovery().await;

// 2. Wait for discovery to find devices
embassy_time::Timer::after(embassy_time::Duration::from_secs(5)).await;

// 3. Get discovered devices
let devices = bondybird::api::get_devices().await.unwrap();

// 4. Connect to the first device
if let Some(device) = devices.first() {
    let addr_str = format_address(device.addr);
    let _ = bondybird::api::connect_device(addr_str.into()).await;
}

// 5. Check Bluetooth state
let state = bondybird::api::get_state().await.unwrap();
```

## Features

- **no_std Compatible**: Designed for embedded systems with minimal resources
- **Async-first Design**: Built on Embassy executor for efficient resource usage
- **Single-task Architecture**: Simplifies memory usage and coordination
- **Static Channels**: Efficient communication with zero heap allocations
- **Direct HCI Integration**: Uses bt-hci for standardized controller communication

## License

The MIT License (MIT)
Copyright © 2025 rttf.dev

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the “Software”), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
