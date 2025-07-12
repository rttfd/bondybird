[<img alt="github" src="https://img.shields.io/badge/github-rttfd/bondybird-37a8e0?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/rttfd/bondybird)
[<img alt="crates.io" src="https://img.shields.io/crates/v/bondybird.svg?style=for-the-badge&color=ff8b94&logo=rust" height="20">](https://crates.io/crates/bondybird)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-bondybird-bedc9c?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/bondybird)

![Dall-E generated bondybird image](https://raw.githubusercontent.com/rttfd/static/refs/heads/main/bondybird/bondybird.png)

# `BondyBird`

`BondyBird` is a BR/EDR (Classic) Bluetooth Host implementation for embedded systems, built on Embassy and bt-hci crates for `no_std` environments. It features a parallel task architecture for optimal performance and responsiveness.

## Key Features

- Parallel processing: HCI events and API requests handled in separate tasks
- Thread-safe: Embassy mutex-based shared state
- Low latency: Immediate HCI event processing
- Memory efficient: heapless collections, `no_std` compatible
- Type safe: Strong typing for addresses
- Async/await API

## Architecture

`BondyBird` uses parallel Embassy tasks:

- HCI Event Processor: Handles incoming HCI events, sends internal commands
- API Request Processor: Handles API requests and responses
- Internal Command Processor: Executes HCI commands triggered by events (no responses)

## Quickstart

```rust,ignore
use bondybird::{processor, BluetoothHostOptions};
use bt_hci::controller::ExternalController;
use embassy_executor::Spawner;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let options = BluetoothHostOptions::default();
    let transport = YourTransport::new();
    let controller = ExternalController::new(transport);
    let controller_ref = Box::leak(Box::new(controller));
    spawner.spawn(processor::run::<YourTransport, 4, 512>(options, controller_ref)).unwrap();
    // Use API functions
    let _ = bondybird::api::start_discovery().await;
    let devices = bondybird::api::get_devices().await.unwrap();
    if let Some(device) = devices.first() {
        let addr_str = device.addr.format_hex();
        bondybird::api::connect_device(&addr_str).await.unwrap();
    }
}
```

## API Usage

- Initialize `BluetoothHost` with options
- Use API functions: `start_discovery`, `get_devices`, `connect_device`, `disconnect_device`, `get_state`

## Example: Complete Bluetooth Flow

```rust,ignore
use bondybird::{processor, BluetoothHostOptions};
use embassy_time::{Duration, Timer};

async fn bluetooth_example() -> Result<(), bondybird::BluetoothError> {
    let options = BluetoothHostOptions {
        lap: [0x33, 0x8B, 0x9E],
        inquiry_length: 8,
        num_responses: 10,
    };
    processor::run::<YourTransport, 4, 512>(options, controller_ref).await;
    bondybird::api::start_discovery().await?;
    Timer::after(Duration::from_secs(5)).await;
    let devices = bondybird::api::get_devices().await?;
    println!("Found {} devices", devices.len());
    if let Some(device) = devices.first() {
        let addr_str = device.addr.format_hex();
        bondybird::api::connect_device(&addr_str).await?;
        println!("Connected to device: {}", addr_str);
    }
    let state = bondybird::api::get_state().await?;
    println!("Bluetooth state: {:?}", state);
    Ok(())
}
```

## License

MIT License © 2025 rttf.dev
