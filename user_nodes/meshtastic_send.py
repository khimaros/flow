def spec():
    return {
        "name": "MeshtasticSend",
        "title": "Meshtastic Send Message",
        "category": "Network",
        "description": "Send a message via Meshtastic",
        "inputs": [
            {
                "name": "connection_type",
                "type": "string",
                "ui": "select",
                "options": [
                    {"label": "Serial", "value": "serial"},
                    {"label": "BLE", "value": "ble"},
                    {"label": "TCP", "value": "tcp"},
                ],
                "required": True,
                "description": "type of connection to Meshtastic device (serial, ble, tcp)",
            },
            {
                "name": "target_device",
                "type": "string",
                "required": True,
                "description": "target device identifier (e.g., /dev/ttyUSB0, MAC address, or IP:Port)",
            },
            {
                "name": "channel",
                "type": "integer",
                "required": True,
                "description": "channel number to send the message on",
            },
            {
                "name": "userid",
                "type": "string",
                "required": False,
                "description": "user ID to send to (sends to all if empty)",
            },
            {
                "name": "message",
                "type": "string",
                "ui": "textarea",
                "required": True,
                "description": "message content to send",
            },
        ],
        "outputs": [
            {
                "name": "status",
                "type": "string",
                "description": "status of the message send operation",
            },
            {
                "name": "error",
                "type": "string",
                "description": "error message if the operation failed",
            },
        ],
    }


def execute(inputs):
    connection_type = inputs["connection_type"]
    target_device = inputs["target_device"]
    channel = inputs["channel"]
    userid = inputs.get("userid")
    message = inputs["message"]

    log(f"Starting Meshtastic Send execution... type={connection_type}")

    # Short-circuit terminal behavior to prevent library interference
    import sys

    if hasattr(sys.stdout, "isatty"):
        sys.stdout.isatty = lambda: False
    if hasattr(sys.stdin, "isatty"):
        sys.stdin.isatty = lambda: False

    interface = None
    log(f"Connecting to device at {target_device}...")
    try:
        if connection_type == "serial":
            import meshtastic.serial_interface

            interface = meshtastic.serial_interface.SerialInterface(target_device)
        elif connection_type == "ble":
            import meshtastic.ble_interface

            interface = meshtastic.ble_interface.BLEInterface(address=target_device)
        elif connection_type == "tcp":
            host, port_str = target_device.split(":")
            import meshtastic.tcp_interface

            interface = meshtastic.tcp_interface.TCPInterface(
                hostname=host, port=int(port_str)
            )

        if userid:
            log(f"sending direct message to user {userid}")
            interface.sendText(
                message, channelIndex=channel, destinationId=userid, wantAck=True
            )
        else:
            log("sending broadcast message")
            interface.sendText(message, channelIndex=channel)

        log("closing interface...")
        interface.close()
        log("interface closed.")

        return {
            "status": "success",
            "error": None,
        }

    except Exception as e:
        log(f"exception during execution: {e}")
        if interface:
            log("closing interface...")
            try:
                interface.close()
                log("interface closed.")
            except Exception as close_e:
                log(f"error closing interface: {close_e}")
        raise e
