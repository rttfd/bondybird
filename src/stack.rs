//! High-level ergonomic `BluetoothStack` facade for `BondyBird`
//!
//! This struct hides all channel setup and provides a simple API for users.

use crate::{
    ApiCommandChannel, ApiCommandSender, ApiResponseChannel, ApiResponseReceiver,
    BluetoothEventHandler, BluetoothManager, CommandChannel, EventChannel,
};
use embassy_sync::channel::Channel;

/// High-level Bluetooth stack facade for ergonomic usage.
///
/// Use [`BluetoothStack::with_control`] to create the stack and obtain a [`BluetoothControl`] handle for API commands.
///
/// # Example
/// ```rust
/// let (mut stack, control) = BluetoothStack::with_control(controller);
/// stack.run().await;
/// // Use `control.api_command(...)` to interact with the stack from another task (e.g., REST API handler)
/// ```
pub struct BluetoothStack<T>
where
    T: bt_hci::controller::Controller
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::controller_baseband::Reset>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::controller_baseband::SetEventMask>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::info::ReadLocalVersionInformation>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::info::ReadLocalSupportedFeatures>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::info::ReadBufferSize>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::info::ReadBdAddr>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::link_control::Inquiry>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::link_control::CreateConnection>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::link_control::Disconnect>
        + 'static,
{
    manager: BluetoothManager<T>,
    handler: BluetoothEventHandler,
}

/// Handle for controlling the Bluetooth stack via external interfaces (e.g., REST API).
pub struct BluetoothControl {
    api_command_sender: ApiCommandSender,
    api_response_receiver: ApiResponseReceiver,
}

impl<T> BluetoothStack<T>
where
    T: bt_hci::controller::Controller
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::controller_baseband::Reset>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::controller_baseband::SetEventMask>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::info::ReadLocalVersionInformation>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::info::ReadLocalSupportedFeatures>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::info::ReadBufferSize>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::info::ReadBdAddr>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::link_control::Inquiry>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::link_control::CreateConnection>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::link_control::Disconnect>
        + 'static,
{
    /// Create a new [`BluetoothStack`] with all channels and tasks set up, returning a [`BluetoothControl`] handle for API commands.
    ///
    /// # Example
    /// ```rust
    /// let (mut stack, control) = BluetoothStack::with_control(controller);
    /// stack.run().await;
    /// // Use `control.api_command(...)` to interact with the stack from another task
    /// ```
    pub fn with_control(controller: T) -> (Self, BluetoothControl) {
        static EVENT_CHANNEL: EventChannel = Channel::new();
        static COMMAND_CHANNEL: CommandChannel = Channel::new();
        static API_COMMAND_CHANNEL: ApiCommandChannel = Channel::new();
        static API_RESPONSE_CHANNEL: ApiResponseChannel = Channel::new();

        let event_sender = EVENT_CHANNEL.sender();
        let event_receiver = EVENT_CHANNEL.receiver();
        let command_sender = COMMAND_CHANNEL.sender();
        let command_receiver = COMMAND_CHANNEL.receiver();

        let api_command_sender = API_COMMAND_CHANNEL.sender();
        let api_command_receiver = API_COMMAND_CHANNEL.receiver();
        let api_response_sender = API_RESPONSE_CHANNEL.sender();
        let api_response_receiver = API_RESPONSE_CHANNEL.receiver();

        let manager = BluetoothManager::new(controller, event_sender, command_receiver);
        let handler = BluetoothEventHandler::new(
            event_receiver,
            command_sender,
            api_command_receiver,
            api_response_sender,
        );

        (
            Self { manager, handler },
            BluetoothControl {
                api_command_sender,
                api_response_receiver,
            },
        )
    }

    /// Start the Bluetooth stack (spawns manager and handler tasks)
    pub async fn run(&mut self) {
        embassy_futures::join::join(self.manager.run(), self.handler.run()).await;
    }
}
