[<img alt="github" src="https://img.shields.io/badge/github-rttfd/bondybird-37a8e0?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/rttfd/bondybird)
[<img alt="crates.io" src="https://img.shields.io/crates/v/bondybird.svg?style=for-the-badge&color=ff8b94&logo=rust" height="20">](https://crates.io/crates/bondybird)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-bondybird-bedc9c?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/bondybird)

![Dall-E generated bondybird image](https://raw.githubusercontent.com/rttfd/static/refs/heads/main/bondybird/bondybird.png)

# BondyBird

BondyBird is a BR/EDR (Classic) Bluetooth Host implementation for embedded systems, built on top of Embassy's async executor and designed for `no_std` environments. It provides a single-task, channel-based architecture that is both memory-efficient and easy to integrate with REST API handlers.

## Architecture

BondyBird uses a simplified architecture with:

1. **Single Manager Task** - A single async task that handles all Bluetooth operations
2. **Static Channels** - Embassy channels for communication between REST handlers and the manager
3. **Direct HCI Integration** - Using the bt-hci library for controller communication

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│  REST Handler   │    │  API REQUEST     │    │                 │
│   Functions     │───▶│    CHANNEL       │───▶│                 │
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
use bondybird::{bluetooth_manager_task, handle_discover, handle_get_devices};
use bt_hci::transport::YourTransport;
use embassy_executor::Spawner;

// Spawn the Bluetooth manager task (only one task needed)
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let transport = YourTransport::new();
    
    // Spawn the single Bluetooth manager task
    spawner.spawn(bluetooth_manager_task(transport)).unwrap();
    
    // Use handler functions in your REST API endpoints
    let _ = handle_discover().await;
    let devices = handle_get_devices().await.unwrap();
}
```

You can use the provided handler functions in your REST API endpoints:
- `handle_discover()` - Start device discovery
- `handle_get_devices()` - Get list of discovered devices
- `handle_pair(address)` - Connect to a device
- `handle_disconnect(address)` - Disconnect from a device
- `handle_get_state()` - Get current Bluetooth state

## Integration with REST API

BondyBird is designed to be easily integrated with REST API handlers. The handler functions are simple and use static channels for communication with the Bluetooth manager task.

```rust,no_run,ignore
// In your REST API handler
async fn rest_discover_endpoint() -> Response {
    match handle_discover().await {
        Ok(()) => Response::new(200, "Discovery started"),
        Err(e) => Response::new(400, format!("Error: {:?}", e)),
    }
}

async fn rest_get_devices_endpoint() -> Response {
    match handle_get_devices().await {
        Ok(devices) => {
            let devices_json = devices.iter().map(|d| {
                // Convert devices to JSON...
            }).collect::<Vec<_>>();
            Response::new(200, serde_json::to_string(&devices_json).unwrap())
        },
        Err(e) => Response::new(400, format!("Error: {:?}", e)),
    }
}
```

## Example: Complete Flow

```rust,no_run,ignore
// 1. Start discovery
let _ = handle_discover().await;

// 2. Wait for discovery to find devices
embassy_time::Timer::after(embassy_time::Duration::from_secs(5)).await;

// 3. Get discovered devices
let devices = handle_get_devices().await.unwrap();

// 4. Connect to the first device
if let Some(device) = devices.first() {
    let addr_str = format_address(device.addr);
    let _ = handle_pair(addr_str.into()).await;
}

// 5. Check Bluetooth state
let state = handle_get_state().await.unwrap();
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
