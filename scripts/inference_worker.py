#!/usr/bin/env python3
"""
ML Inference Worker for Unhidra

This script acts as a sidecar inference server, communicating with the Rust
parent process via Unix Domain Sockets (UDS) using length-prefixed JSON messages.

Protocol:
- Request: 4-byte big-endian length + JSON payload
- Response: 4-byte big-endian length + JSON payload

Usage:
    python3 inference_worker.py /tmp/unhidra_ml_infer.sock

The worker runs an asyncio event loop and handles requests sequentially.
For production use with higher throughput needs, consider spawning multiple
worker processes.

Author: Unhidra Team
License: MIT
"""

import asyncio
import json
import os
import signal
import sys
import time
from typing import Any


class InferenceServer:
    """Async UNIX socket server for ML inference requests."""

    def __init__(self, socket_path: str):
        """
        Initialize the inference server.

        Args:
            socket_path: Path to the Unix domain socket
        """
        self.socket_path = socket_path
        self.running = True
        self.request_count = 0
        self.total_processing_time_ms = 0

    async def perform_inference(self, data: str, model_id: str | None = None) -> str:
        """
        Perform ML inference on the input data.

        This is a mock implementation that simulates ML computation.
        In production, replace this with actual ML model inference.

        Args:
            data: Input data for inference
            model_id: Optional model identifier for multi-model deployments

        Returns:
            Inference result as a string
        """
        # Simulate ML computation delay (500ms)
        # In production, replace with actual model inference
        await asyncio.sleep(0.5)

        # Mock inference logic - echo back with "_ok" suffix
        # Replace this with actual ML model calls:
        # - Load model (can be done once at startup)
        # - Preprocess input data
        # - Run model inference
        # - Postprocess and return results
        return f"{data}_ok"

    async def handle_client(
        self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ):
        """
        Handle a single client connection (the Rust parent process).

        Reads length-prefixed JSON requests, processes them, and sends
        length-prefixed JSON responses.

        Args:
            reader: Async stream reader for incoming data
            writer: Async stream writer for outgoing data
        """
        peer = writer.get_extra_info("peername")
        print(f"[*] Client connected: {peer}", file=sys.stderr)

        try:
            while self.running:
                # Read exactly 4 bytes for message length
                length_data = await reader.readexactly(4)
                msg_length = int.from_bytes(length_data, byteorder="big")

                if msg_length == 0:
                    print("[!] Received zero-length message, ignoring", file=sys.stderr)
                    continue

                if msg_length > 10 * 1024 * 1024:  # 10 MB limit
                    print(
                        f"[!] Message too large ({msg_length} bytes), disconnecting",
                        file=sys.stderr,
                    )
                    break

                # Read the full JSON message of the given length
                msg_bytes = await reader.readexactly(msg_length)
                request = json.loads(msg_bytes.decode("utf-8"))

                # Extract request fields
                data = request.get("data", "")
                request_id = request.get("request_id")
                model_id = request.get("model_id")

                self.request_count += 1
                print(
                    f"[>] Request #{self.request_count}: data_len={len(data)}, "
                    f"request_id={request_id}, model_id={model_id}",
                    file=sys.stderr,
                )

                # Perform inference and measure time
                start_time = time.perf_counter()
                try:
                    result = await self.perform_inference(data, model_id)
                    error = None
                except Exception as e:
                    result = ""
                    error = str(e)
                    print(f"[!] Inference error: {e}", file=sys.stderr)

                processing_time_ms = int((time.perf_counter() - start_time) * 1000)
                self.total_processing_time_ms += processing_time_ms

                # Build response
                response_obj: dict[str, Any] = {
                    "result": result,
                }

                # Include optional fields only if they have values
                if request_id:
                    response_obj["request_id"] = request_id
                if processing_time_ms:
                    response_obj["processing_time_ms"] = processing_time_ms
                if error:
                    response_obj["error"] = error

                response_bytes = json.dumps(response_obj).encode("utf-8")

                # Send response length and payload
                writer.write(len(response_bytes).to_bytes(4, byteorder="big"))
                writer.write(response_bytes)
                await writer.drain()

                print(
                    f"[<] Response: result_len={len(result)}, "
                    f"processing_time={processing_time_ms}ms",
                    file=sys.stderr,
                )

        except asyncio.IncompleteReadError:
            # Client disconnected cleanly
            print("[*] Client disconnected", file=sys.stderr)
        except json.JSONDecodeError as e:
            print(f"[!] JSON decode error: {e}", file=sys.stderr)
        except Exception as e:
            print(f"[!] Unexpected error handling client: {e}", file=sys.stderr)
        finally:
            writer.close()
            await writer.wait_closed()
            print(
                f"[*] Connection closed. Total requests: {self.request_count}, "
                f"Total processing time: {self.total_processing_time_ms}ms",
                file=sys.stderr,
            )

    async def run(self):
        """
        Start the inference server and listen for connections.

        Removes any existing socket file to avoid bind errors, then creates
        a Unix domain socket server and waits for connections.
        """
        # Remove existing socket file if any to avoid bind errors
        try:
            os.unlink(self.socket_path)
        except FileNotFoundError:
            pass

        # Create the Unix domain socket server
        server = await asyncio.start_unix_server(
            self.handle_client, path=self.socket_path
        )

        # Set socket file permissions (owner read/write only for security)
        os.chmod(self.socket_path, 0o600)

        print(f"[*] Inference server listening on {self.socket_path}", file=sys.stderr)
        print(
            f"[*] Protocol: length-prefixed JSON (4-byte big-endian + payload)",
            file=sys.stderr,
        )
        print(f"[*] PID: {os.getpid()}", file=sys.stderr)

        # Setup graceful shutdown handlers
        loop = asyncio.get_event_loop()

        def shutdown_handler():
            print("\n[*] Shutdown signal received", file=sys.stderr)
            self.running = False
            server.close()

        for sig in (signal.SIGTERM, signal.SIGINT):
            loop.add_signal_handler(sig, shutdown_handler)

        async with server:
            try:
                await server.serve_forever()
            except asyncio.CancelledError:
                print("[*] Server cancelled", file=sys.stderr)

        print("[*] Server shutdown complete", file=sys.stderr)


def main():
    """Main entry point for the inference worker."""
    # Socket path provided by parent process via command-line argument
    if len(sys.argv) > 1:
        socket_path = sys.argv[1]
    else:
        socket_path = "/tmp/unhidra_ml_infer.sock"
        print(
            f"[!] No socket path provided, using default: {socket_path}",
            file=sys.stderr,
        )

    print(f"[*] Starting Unhidra ML Inference Worker", file=sys.stderr)
    print(f"[*] Python version: {sys.version}", file=sys.stderr)

    server = InferenceServer(socket_path)

    try:
        asyncio.run(server.run())
    except KeyboardInterrupt:
        print("\n[*] Interrupted by user", file=sys.stderr)
    except Exception as e:
        print(f"[!] Fatal error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
