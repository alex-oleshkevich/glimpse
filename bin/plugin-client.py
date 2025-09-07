#!/usr/bin/env python3
# /// script
# dependencies = ["textual>=0.44.0", "anyio>=4.0.0"]
# ///

"""
Interactive Plugin Debugger TUI

A Terminal User Interface for debugging and testing Glimpse plugins.
Provides real-time message exchange, smart command generation, and
comprehensive plugin lifecycle management.

Installation (PEP-772):
    pip install textual>=0.44.0 anyio>=4.0.0

Usage:
    ./bin/plugin-client.py cargo run -p glimpse-plugins-echo
    ./bin/plugin-client.py /path/to/your/plugin

Features:
    [S] Search Query - Enter search terms, auto-generates request JSON
    [C] Cancel - Send cancel notification to plugin (when requests pending)
    [Q] Quit - Send quit notification to plugin
    [J] Custom JSON - Edit and send custom JSON messages
    [R] Repeat Last - Repeat the last sent message
    Ctrl+L - Clear message history
    Ctrl+Q - Exit the debugger

The TUI shows:
    - Real-time message exchange with timestamps
    - Response times for requests
    - Raw JSON for sent/received messages
    - Plugin status and pending request tracking
    - Message history with color coding
"""

import asyncio
import json
import subprocess
import sys
import time
from datetime import datetime
from typing import Any, Dict, List

import anyio
from textual import on
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Container, Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import (
    Button, Footer, Header, Input, Label, Log, Pretty,
    TextArea
)
from textual.reactive import reactive


class MessageHistory:
    """Manages plugin message history and statistics"""

    def __init__(self):
        self.messages: List[Dict[str, Any]] = []
        self.pending_requests: Dict[int, float] = {}  # id -> timestamp
        self.next_request_id = 1

    def add_sent_message(self, message: Dict[str, Any]) -> None:
        """Add a sent message to history"""
        timestamp = time.time()
        entry = {
            "type": "sent",
            "message": message,
            "timestamp": timestamp,
            "formatted_time": datetime.fromtimestamp(timestamp).strftime("%H:%M:%S")
        }

        # Track pending requests
        if "id" in message:
            self.pending_requests[message["id"]] = timestamp

        self.messages.append(entry)

    def add_received_message(self, message: Dict[str, Any]) -> None:
        """Add a received message to history"""
        timestamp = time.time()
        response_time = None

        # Calculate response time for requests
        if "id" in message and message["id"] in self.pending_requests:
            request_time = self.pending_requests.pop(message["id"])
            response_time = (timestamp - request_time) * 1000  # Convert to ms

        entry = {
            "type": "received",
            "message": message,
            "timestamp": timestamp,
            "formatted_time": datetime.fromtimestamp(timestamp).strftime("%H:%M:%S"),
            "response_time": response_time
        }

        self.messages.append(entry)

    def get_next_request_id(self) -> int:
        """Get the next available request ID"""
        request_id = self.next_request_id
        self.next_request_id += 1
        return request_id

    def get_recent_searches(self, limit: int = 5) -> List[str]:
        """Get recent search queries"""
        searches = []
        for entry in reversed(self.messages):
            if (entry["type"] == "sent" and
                entry["message"].get("method") == "search" and
                "params" in entry["message"]):
                query = entry["message"]["params"]
                if query not in searches:
                    searches.append(query)
                    if len(searches) >= limit:
                        break
        return searches

    def has_pending_requests(self) -> bool:
        """Check if there are pending requests"""
        return len(self.pending_requests) > 0

    def get_pending_request_ids(self) -> List[int]:
        """Get list of pending request IDs"""
        return list(self.pending_requests.keys())


class SearchDialog(ModalScreen[str]):
    """Modal dialog for entering search queries"""

    def __init__(self, recent_searches: List[str]):
        super().__init__()
        self.recent_searches = recent_searches

    def compose(self) -> ComposeResult:
        with Container(id="search-dialog"):
            yield Label("Enter Search Query", id="search-title")
            yield Input(placeholder="Type your search query...", id="search-input")

            if self.recent_searches:
                yield Label("Recent Searches:", id="recent-label")
                for i, search in enumerate(self.recent_searches[:5], 1):
                    yield Button(f"[{i}] {search}", id=f"recent-{i}", classes="recent-search")

            with Horizontal(id="search-buttons"):
                yield Button("Send", variant="primary", id="search-send")
                yield Button("Cancel", id="search-cancel")

    @on(Button.Pressed, "#search-send")
    @on(Input.Submitted, "#search-input")
    async def send_search(self, event):
        query = self.query_one("#search-input", Input).value.strip()
        if query:
            self.dismiss(query)

    @on(Button.Pressed, "#search-cancel")
    async def cancel_search(self, event):
        self.dismiss(None)

    @on(Button.Pressed, ".recent-search")
    async def select_recent(self, event):
        # Extract search text from button label
        button_text = event.button.label.plain
        search_text = button_text[4:]  # Remove "[N] " prefix
        self.dismiss(search_text)


class CustomJsonDialog(ModalScreen[str]):
    """Modal dialog for entering custom JSON messages"""

    def __init__(self, next_request_id: int):
        super().__init__()
        self.next_request_id = next_request_id

    def compose(self) -> ComposeResult:
        with Container(id="json-dialog"):
            yield Label("Custom JSON Message", id="json-title")

            with Horizontal(id="json-templates"):
                yield Button("Request", id="template-request")
                yield Button("Notification", id="template-notification")
                yield Button("Empty", id="template-empty")

            yield TextArea(
                text=self._get_request_template(),
                language="json",
                id="json-editor"
            )

            yield Label("", id="json-validation")

            with Horizontal(id="json-buttons"):
                yield Button("Send", variant="primary", id="json-send")
                yield Button("Validate", id="json-validate")
                yield Button("Cancel", id="json-cancel")

    def _get_request_template(self) -> str:
        """Get template for request message"""
        return json.dumps({
            "id": self.next_request_id,
            "method": "",
            "params": ""
        }, indent=2)

    def _get_notification_template(self) -> str:
        """Get template for notification message"""
        return json.dumps({
            "method": ""
        }, indent=2)

    def _get_empty_template(self) -> str:
        """Get empty JSON template"""
        return "{}"

    @on(Button.Pressed, "#template-request")
    async def use_request_template(self, event):
        editor = self.query_one("#json-editor", TextArea)
        editor.text = self._get_request_template()

    @on(Button.Pressed, "#template-notification")
    async def use_notification_template(self, event):
        editor = self.query_one("#json-editor", TextArea)
        editor.text = self._get_notification_template()

    @on(Button.Pressed, "#template-empty")
    async def use_empty_template(self, event):
        editor = self.query_one("#json-editor", TextArea)
        editor.text = self._get_empty_template()

    @on(Button.Pressed, "#json-validate")
    async def validate_json(self, event):
        editor = self.query_one("#json-editor", TextArea)
        validation_label = self.query_one("#json-validation", Label)

        try:
            json.loads(editor.text)
            validation_label.update("✓ Valid JSON")
            validation_label.add_class("valid")
            validation_label.remove_class("invalid")
        except json.JSONDecodeError as e:
            validation_label.update(f"✗ Invalid JSON: {str(e)}")
            validation_label.add_class("invalid")
            validation_label.remove_class("valid")

    @on(Button.Pressed, "#json-send")
    async def send_json(self, event):
        editor = self.query_one("#json-editor", TextArea)
        try:
            # Validate JSON before sending
            json.loads(editor.text)
            self.dismiss(editor.text)
        except json.JSONDecodeError as e:
            validation_label = self.query_one("#json-validation", Label)
            validation_label.update(f"✗ Invalid JSON: {str(e)}")
            validation_label.add_class("invalid")

    @on(Button.Pressed, "#json-cancel")
    async def cancel_json(self, event):
        self.dismiss(None)


class PluginDebuggerApp(App):
    """Main TUI application for plugin debugging"""

    CSS = """
    #main-container {
        layout: horizontal;
        height: 100%;
    }

    #left-panel {
        width: 2fr;
        border: solid white;
        margin: 1;
    }

    #middle-panel {
        width: 1fr;
        border: solid white;
        margin: 1;
    }

    #right-panel {
        width: 1fr;
        layout: vertical;
        margin: 1;
    }

    #message-history {
        height: 1fr;
        border: solid white;
        margin-bottom: 1;
    }

    #plugin-logs {
        height: 1fr;
        border: solid white;
    }

    #raw-json {
        height: 100%;
        border: solid white;
    }

    #actions-panel {
        height: 20;
        border: solid white;
        margin-bottom: 1;
    }

    #status-panel {
        height: 8;
        border: solid white;
    }

    .action-button {
        width: 100%;
        margin-bottom: 1;
    }

    .recent-search {
        width: 100%;
        margin: 0 1;
    }

    .disabled {
        opacity: 0.5;
    }

    #search-dialog, #json-dialog {
        align: center middle;
        width: 60%;
        height: 70%;
        background: black;
        border: thick white;
    }

    #search-input {
        margin: 1;
    }

    #json-editor {
        height: 1fr;
        margin: 1;
    }

    #search-buttons, #json-buttons {
        align: center middle;
        height: auto;
    }

    #json-templates {
        align: center middle;
        height: auto;
        margin: 1;
    }
    """

    BINDINGS = [
        Binding("s", "search", "Search", priority=True),
        Binding("c", "cancel", "Cancel", priority=True),
        Binding("q", "quit_plugin", "Quit Plugin", priority=True),
        Binding("j", "custom_json", "Custom JSON", priority=True),
        Binding("r", "repeat_last", "Repeat Last", priority=True),
        Binding("ctrl+l", "clear_history", "Clear History", priority=True),
        Binding("ctrl+q", "quit", "Exit App", priority=True),
    ]

    plugin_status = reactive("Stopped")
    current_request_id = reactive(1)

    def __init__(self, plugin_command: List[str]):
        super().__init__()
        self.plugin_command = plugin_command
        self.message_history = MessageHistory()
        self.plugin_process = None
        self.last_sent_message = None
        self.last_received_message = None

    def compose(self) -> ComposeResult:
        yield Header()

        with Container(id="main-container"):
            # Left panel - Message history and plugin logs
            with Vertical(id="left-panel"):
                with Container(id="message-history"):
                    yield Label("Message History", id="history-title")
                    yield Log(id="message-log")

                with Container(id="plugin-logs"):
                    yield Label("Plugin Logs", id="logs-title")
                    yield Log(id="plugin-log")

            # Middle panel - Raw JSON
            with Vertical(id="middle-panel"):
                with Container(id="raw-json"):
                    yield Label("Raw JSON", id="json-title")
                    yield Pretty(None, id="last-sent")
                    yield Pretty(None, id="last-received")

            # Right panel - Actions and status
            with Vertical(id="right-panel"):
                with Container(id="actions-panel"):
                    yield Label("Quick Actions", id="actions-title")
                    yield Button("🔍 [S] Search Query", id="action-search", classes="action-button")
                    yield Button("🚫 [C] Cancel Current", id="action-cancel", classes="action-button disabled")
                    yield Button("🛑 [Q] Quit Plugin", id="action-quit", classes="action-button")
                    yield Button("📝 [J] Custom JSON", id="action-json", classes="action-button")
                    yield Button("🔄 [R] Repeat Last", id="action-repeat", classes="action-button")

                with Container(id="status-panel"):
                    yield Label("Plugin Status", id="status-title")
                    yield Label(f"Command: {' '.join(self.plugin_command)}", id="status-command")
                    yield Label("Status: Stopped", id="status-state")
                    yield Label("Next Request ID: 1", id="status-request-id")
                    yield Label("Pending Requests: 0", id="status-pending")

        yield Footer()

    async def on_mount(self) -> None:
        """Initialize the application"""
        # Start the plugin
        await self._start_plugin()

        # Update UI
        self._update_status_display()

    async def _start_plugin(self) -> None:
        """Start the plugin subprocess"""
        try:
            self.plugin_process = await anyio.open_process(
                self.plugin_command,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE
            )

            self.plugin_status = "Running"
            self._log_message("Plugin started successfully", "system")

            # Start background tasks to read plugin output and errors
            asyncio.create_task(self._read_plugin_output())
            asyncio.create_task(self._read_plugin_errors())

        except Exception as e:
            self.plugin_status = "Failed"
            self._log_message(f"Failed to start plugin: {e}", "error")

    async def _read_plugin_output(self) -> None:
        """Read output from the plugin subprocess"""
        if not self.plugin_process:
            return

        try:
            while True:
                try:
                    line = await self.plugin_process.stdout.receive(8192)
                    if not line:
                        # Plugin has closed stdout - normal termination
                        self.plugin_status = "Disconnected"
                        self._log_message("Plugin disconnected (stdout closed)", "system")
                        self._update_status_display()
                        break

                    line_str = line.decode().strip()
                    if line_str:
                        try:
                            message = json.loads(line_str)
                            await self._handle_received_message(message)
                        except json.JSONDecodeError as e:
                            self._log_message(f"Invalid JSON from plugin: {e}", "error")
                            self._log_message(f"Raw output: {line_str}", "debug")
                            
                except anyio.EndOfStream:
                    # Normal plugin termination
                    self.plugin_status = "Disconnected"
                    self._log_message("Plugin disconnected", "system")
                    self._update_status_display()
                    break
                except anyio.BrokenResourceError:
                    # Plugin process was terminated
                    self.plugin_status = "Terminated"
                    self._log_message("Plugin process was terminated", "system")
                    self._update_status_display()
                    break
                    
        except Exception as e:
            self._log_message(f"Error reading plugin output: {e}", "error")
            self.plugin_status = "Crashed"
            self._update_status_display()

    async def _read_plugin_errors(self) -> None:
        """Read stderr from the plugin subprocess"""
        if not self.plugin_process:
            return

        try:
            while True:
                try:
                    line = await self.plugin_process.stderr.receive(8192)
                    if not line:
                        break

                    line_str = line.decode().strip()
                    if line_str:
                        self._log_plugin_message(line_str)
                        
                except anyio.EndOfStream:
                    break
                except anyio.BrokenResourceError:
                    break
                    
        except Exception as e:
            self._log_plugin_message(f"Error reading plugin stderr: {e}")

    async def _send_message(self, message: Dict[str, Any]) -> None:
        """Send a message to the plugin"""
        if not self.plugin_process:
            self._log_message("No plugin process available", "error")
            return

        if self.plugin_status in ["Disconnected", "Terminated", "Crashed", "Failed"]:
            self._log_message(f"Cannot send message: plugin is {self.plugin_status.lower()}", "error")
            return

        try:
            json_str = json.dumps(message) + "\n"
            await self.plugin_process.stdin.send(json_str.encode())

            # Update history and UI
            self.message_history.add_sent_message(message)
            self.last_sent_message = message
            self._log_message(f"→ Sent: {json.dumps(message, separators=(',', ':'))}", "sent")
            self._update_raw_json_display()
            self._update_status_display()

        except (anyio.BrokenResourceError, anyio.EndOfStream) as e:
            self._log_message("Plugin disconnected while sending message", "error")
            self.plugin_status = "Disconnected"
            self._update_status_display()
        except Exception as e:
            self._log_message(f"Error sending message: {e}", "error")

    async def _handle_received_message(self, message: Dict[str, Any]) -> None:
        """Handle a message received from the plugin"""
        self.message_history.add_received_message(message)
        self.last_received_message = message

        # Calculate response time if available
        response_time_str = ""
        if self.message_history.messages:
            last_entry = self.message_history.messages[-1]
            if last_entry.get("response_time"):
                response_time_str = f" ({last_entry['response_time']:.0f}ms)"

        self._log_message(f"← Received{response_time_str}: {json.dumps(message, separators=(',', ':'))}", "received")
        self._update_raw_json_display()
        self._update_status_display()

    def _log_message(self, message: str, msg_type: str = "info") -> None:
        """Add a message to the log with timestamp"""
        timestamp = datetime.now().strftime("%H:%M:%S")
        log_widget = self.query_one("#message-log", Log)

        # Format long JSON messages with proper indentation
        if msg_type in ["sent", "received"] and message.startswith(("→ Sent:", "← Received")):
            # Extract the JSON part and format it
            prefix = message.split(": ", 1)[0] + ":"
            try:
                json_part = message.split(": ", 1)[1]
                # Remove response time info if present
                if ")" in json_part and json_part.endswith(")"):
                    response_time = json_part[json_part.rfind("("):]
                    json_str = json_part[:json_part.rfind("(")].strip()
                    prefix += response_time
                else:
                    json_str = json_part
                
                # Parse and reformat JSON
                parsed = json.loads(json_str)
                formatted_json = json.dumps(parsed, indent=2)
                
                # Split into lines and display each line separately
                lines = formatted_json.split('\n')
                
                # First line with timestamp and prefix
                if msg_type == "sent":
                    log_widget.write(f"{timestamp} [blue]{prefix}[/blue] {lines[0]}")
                else:
                    log_widget.write(f"{timestamp} [green]{prefix}[/green] {lines[0]}")
                
                # Add remaining lines with proper indentation
                for line in lines[1:]:
                    if line.strip():  # Skip empty lines
                        log_widget.write(f"    {line}")
                return
            except (json.JSONDecodeError, IndexError):
                # Fall back to original message if JSON parsing fails
                pass

        # Color-code messages based on type
        if msg_type == "sent":
            log_widget.write(f"{timestamp} [blue]{message}[/blue]")
        elif msg_type == "received":
            log_widget.write(f"{timestamp} [green]{message}[/green]")
        elif msg_type == "error":
            log_widget.write(f"{timestamp} [red]⚠ {message}[/red]")
        elif msg_type == "system":
            log_widget.write(f"{timestamp} [yellow]ℹ {message}[/yellow]")
        else:
            log_widget.write(f"{timestamp} {message}")

    def _log_plugin_message(self, message: str) -> None:
        """Add a message to the plugin log panel"""
        timestamp = datetime.now().strftime("%H:%M:%S")
        plugin_log_widget = self.query_one("#plugin-log", Log)
        plugin_log_widget.write(f"{timestamp} [yellow]{message}[/yellow]")

    def _update_raw_json_display(self) -> None:
        """Update the raw JSON display panels"""
        try:
            sent_widget = self.query_one("#last-sent", Pretty)
            received_widget = self.query_one("#last-received", Pretty)

            if self.last_sent_message:
                sent_widget.update(self.last_sent_message)
                self._log_message(f"Updated sent widget with: {type(self.last_sent_message)}", "system")

            if self.last_received_message:
                received_widget.update(self.last_received_message)
                self._log_message(f"Updated received widget with: {type(self.last_received_message)}", "system")
        except Exception as e:
            self._log_message(f"Error updating raw JSON display: {e}", "error")

    def _update_status_display(self) -> None:
        """Update the status panel"""
        status_state = self.query_one("#status-state", Label)
        status_request_id = self.query_one("#status-request-id", Label)
        status_pending = self.query_one("#status-pending", Label)
        cancel_button = self.query_one("#action-cancel", Button)

        status_state.update(f"Status: {self.plugin_status}")
        status_request_id.update(f"Next Request ID: {self.message_history.get_next_request_id()}")

        pending_ids = self.message_history.get_pending_request_ids()
        status_pending.update(f"Pending Requests: {len(pending_ids)}")

        # Disable all action buttons if plugin is not running
        plugin_inactive = self.plugin_status in ["Disconnected", "Terminated", "Crashed", "Failed", "Stopped"]
        
        # Update action buttons based on plugin status
        action_buttons = ["#action-search", "#action-quit", "#action-json", "#action-repeat"]
        for button_id in action_buttons:
            button = self.query_one(button_id, Button)
            if plugin_inactive:
                button.add_class("disabled")
            else:
                button.remove_class("disabled")

        # Enable/disable cancel button based on pending requests and plugin status
        if self.message_history.has_pending_requests() and not plugin_inactive:
            cancel_button.remove_class("disabled")
            cancel_button.label = f"🚫 [C] Cancel Current ({', '.join(map(str, pending_ids))})"
        else:
            cancel_button.add_class("disabled")
            cancel_button.label = "🚫 [C] Cancel Current"

    # Action handlers
    async def action_search(self) -> None:
        """Open search dialog"""
        self._log_message("Opening search dialog", "system")
        recent_searches = self.message_history.get_recent_searches()
        result = await self.push_screen(SearchDialog(recent_searches))
        
        self._log_message(f"Search dialog result: {repr(result)}", "system")
        if result:
            message = {
                "id": self.message_history.get_next_request_id(),
                "method": "search",
                "params": result
            }
            self._log_message(f"Sending search message: {message}", "system")
            await self._send_message(message)
        else:
            self._log_message("Search dialog returned no result", "system")

    async def action_cancel(self) -> None:
        """Send cancel command"""
        if not self.message_history.has_pending_requests():
            self._log_message("No pending requests to cancel", "warning")
            return

        message = {"method": "cancel"}
        await self._send_message(message)

    async def action_quit_plugin(self) -> None:
        """Send quit command to plugin"""
        message = {"method": "quit"}
        await self._send_message(message)

        # Give plugin time to respond, then close
        await anyio.sleep(1)
        if self.plugin_process:
            self.plugin_process.terminate()
            self.plugin_status = "Stopped"
            self._update_status_display()

    async def action_custom_json(self) -> None:
        """Open custom JSON dialog"""
        result = await self.push_screen(CustomJsonDialog(self.message_history.get_next_request_id()))

        if result:
            try:
                message = json.loads(result)
                await self._send_message(message)
            except json.JSONDecodeError as e:
                self._log_message(f"Invalid JSON: {e}", "error")

    async def action_repeat_last(self) -> None:
        """Repeat the last sent message"""
        if not self.last_sent_message:
            self._log_message("No previous message to repeat", "warning")
            return

        # Create a copy and update ID if it was a request
        message = self.last_sent_message.copy()
        if "id" in message:
            message["id"] = self.message_history.get_next_request_id()

        await self._send_message(message)

    async def action_clear_history(self) -> None:
        """Clear message history"""
        log_widget = self.query_one("#message-log", Log)
        log_widget.clear()
        self.message_history.messages.clear()
        self._log_message("Message history cleared", "system")

    # Button event handlers
    @on(Button.Pressed, "#action-search")
    async def on_search_button(self, event):
        self._log_message("Search button pressed", "system")
        if not event.button.has_class("disabled"):
            self._log_message("Search button is enabled, calling action_search", "system")
            await self.action_search()
        else:
            self._log_message("Search button is disabled", "system")

    @on(Button.Pressed, "#action-cancel")
    async def on_cancel_button(self, event):
        if not event.button.has_class("disabled"):
            await self.action_cancel()

    @on(Button.Pressed, "#action-quit")
    async def on_quit_button(self, event):
        if not event.button.has_class("disabled"):
            await self.action_quit_plugin()

    @on(Button.Pressed, "#action-json")
    async def on_json_button(self, event):
        if not event.button.has_class("disabled"):
            await self.action_custom_json()

    @on(Button.Pressed, "#action-repeat")
    async def on_repeat_button(self, event):
        if not event.button.has_class("disabled"):
            await self.action_repeat_last()


async def main():
    """Main entry point"""
    if len(sys.argv) < 2:
        print("Usage: plugin-client.py <plugin-command> [args...]")
        print("Example: plugin-client.py cargo run -p glimpse-plugins-echo")
        sys.exit(1)

    plugin_command = sys.argv[1:]
    
    # Check if we're in a proper terminal
    if not sys.stdout.isatty():
        print("Error: This tool requires an interactive terminal")
        sys.exit(1)
    
    app = PluginDebuggerApp(plugin_command)
    
    try:
        await app.run_async()
    except Exception as e:
        print(f"TUI Error: {e}")
        print("Try running in a different terminal or with TERM=xterm-256color")
        sys.exit(1)


if __name__ == "__main__":
    anyio.run(main)
