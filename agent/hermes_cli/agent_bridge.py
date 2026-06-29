#!/usr/bin/env python3
"""
Agent Bridge - JSON-RPC 2.0 server for Rust Gateway ↔ Python Agent communication.

This module implements the agent side of the bridge protocol, reading JSON-RPC
requests from stdin and sending responses/notifications to stdout.

Protocol: JSON-RPC 2.0 over stdin/stdout (line-delimited)
Transport: Each message is a single line ending with \n
"""

import asyncio
import json
import logging
import os
import sys
import threading
import traceback
from typing import Any, Dict, Optional

# Set up logging to stderr (stdout is reserved for JSON-RPC protocol)
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    stream=sys.stderr,
)
logger = logging.getLogger(__name__)


class AgentBridgeServer:
    """
    JSON-RPC 2.0 server that manages agent sessions and processes messages.

    The server maintains a mapping of session_id → agent instance and handles
    incoming requests from the Rust gateway.
    """

    def __init__(self):
        self.sessions: Dict[str, Any] = {}
        self.interrupt_flags: Dict[str, threading.Event] = {}

    async def run(self):
        """Main event loop - read stdin line by line and process requests."""
        logger.info("Agent bridge server starting...")
        sys.stderr.flush()

        while True:
            try:
                line = sys.stdin.readline()
                if not line:
                    logger.info("stdin closed, exiting")
                    break

                await self.handle_request(line.strip())

            except KeyboardInterrupt:
                logger.info("Received interrupt signal, exiting")
                break
            except Exception as e:
                logger.error(f"Error in main loop: {e}\n{traceback.format_exc()}")
                sys.stderr.flush()

    async def handle_request(self, line: str):
        """Parse and dispatch JSON-RPC request."""
        if not line:
            return

        try:
            req = json.loads(line)
            method = req.get("method")
            params = req.get("params", {})
            req_id = req.get("id")

            logger.debug(f"Received request: method={method} id={req_id}")

            # Dispatch to handler
            if method == "start_session":
                result = await self.start_session(**params)
            elif method == "handle_message":
                result = await self.handle_message(**params)
            elif method == "interrupt":
                result = await self.interrupt(**params)
            elif method == "end_session":
                result = await self.end_session(**params)
            elif method == "ping":
                result = await self.ping(**params)
            else:
                raise Exception(f"Unknown method: {method}")

            # Send response
            self.send_response(req_id, result)

        except json.JSONDecodeError as e:
            logger.error(f"Invalid JSON: {e}")
            self.send_error(None, -32700, f"Parse error: {e}")
        except Exception as e:
            logger.error(f"Request handler error: {e}\n{traceback.format_exc()}")
            self.send_error(req.get("id") if "req" in locals() else None, -32603, str(e))

    def send_response(self, req_id: Any, result: Any):
        """Send JSON-RPC response to stdout."""
        resp = {
            "jsonrpc": "2.0",
            "result": result,
            "id": req_id
        }
        print(json.dumps(resp), flush=True)
        logger.debug(f"Sent response: id={req_id}")

    def send_error(self, req_id: Any, code: int, message: str):
        """Send JSON-RPC error response to stdout."""
        resp = {
            "jsonrpc": "2.0",
            "error": {
                "code": code,
                "message": message
            },
            "id": req_id
        }
        print(json.dumps(resp), flush=True)
        logger.debug(f"Sent error: id={req_id} code={code}")

    def send_notification(self, method: str, params: Dict[str, Any]):
        """Send JSON-RPC notification to stdout (no id, no response expected)."""
        notif = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        }
        print(json.dumps(notif), flush=True)
        logger.debug(f"Sent notification: method={method}")

    async def ping(self, **kwargs) -> Dict[str, Any]:
        """Health check endpoint."""
        return {
            "status": "alive",
            "sessions": len(self.sessions)
        }

    async def start_session(
        self,
        session_id: str,
        platform: str,
        chat_id: str,
        user_id: str,
        config: Dict[str, Any]
    ) -> Dict[str, Any]:
        """
        Initialize a new agent session.

        Creates an AIAgent instance with the specified configuration and
        stores it in the sessions map.
        """
        logger.info(f"Starting session: {session_id} platform={platform}")

        try:
            # Create a simple session object
            # TODO: Initialize actual AIAgent instance
            session = {
                "session_id": session_id,
                "platform": platform,
                "chat_id": chat_id,
                "user_id": user_id,
                "config": config,
                "conversation_history": [],
                "tools_loaded": 0,  # TODO: load actual tools
            }

            self.sessions[session_id] = session
            self.interrupt_flags[session_id] = threading.Event()

            logger.info(f"Session created: {session_id}")

            return {
                "status": "ready",
                "session_id": session_id,
                "loaded_tools": session["tools_loaded"],
                "memory_snapshots": 0  # TODO: count actual memory snapshots
            }

        except Exception as e:
            logger.error(f"Failed to start session {session_id}: {e}\n{traceback.format_exc()}")
            raise

    async def handle_message(
        self,
        session_id: str,
        text: str,
        attachments: Optional[list] = None,
        reply_to_message_id: Optional[str] = None
    ) -> Dict[str, Any]:
        """
        Handle an inbound user message - runs agent loop with streaming.

        This is the core method that processes user input and generates
        agent responses. It sends notifications for typing indicators,
        streaming text chunks, tool executions, and completion.
        """
        logger.info(f"Handling message for session {session_id}: {text[:50]}...")

        session = self.sessions.get(session_id)
        if not session:
            raise Exception(f"Session not found: {session_id}")

        chat_id = session["chat_id"]

        try:
            # Send typing indicator
            self.send_notification("typing_start", {
                "session_id": session_id,
                "chat_id": chat_id
            })

            # Generate a message ID for tracking
            message_id = f"msg_{session_id}_{len(session['conversation_history'])}"

            # TODO: Run actual agent loop with streaming
            # For now, send a placeholder response

            # Simulate streaming response
            response_text = f"收到消息: {text}\n\n这是一个占位响应。需要实现完整的 agent loop。"

            # Stream chunks
            for i, chunk in enumerate(response_text.split()):
                self.send_notification("stream_chunk", {
                    "session_id": session_id,
                    "chat_id": chat_id,
                    "text": chunk + " ",
                    "is_final": False
                })
                await asyncio.sleep(0.05)  # Simulate streaming delay

            # Send final completion
            self.send_notification("message_complete", {
                "session_id": session_id,
                "chat_id": chat_id,
                "text": response_text,
                "metadata": {
                    "tokens": {"input": 0, "output": 0},
                    "model": session["config"].get("model", "unknown"),
                    "duration_ms": 0
                }
            })

            # Store in conversation history
            session["conversation_history"].append({
                "role": "user",
                "content": text
            })
            session["conversation_history"].append({
                "role": "assistant",
                "content": response_text
            })

            logger.info(f"Message processed for session {session_id}")

            # Return immediate acknowledgment
            return {
                "status": "processing",
                "message_id": message_id
            }

        except Exception as e:
            logger.error(f"Error handling message for {session_id}: {e}\n{traceback.format_exc()}")

            # Send error notification
            self.send_notification("error", {
                "session_id": session_id,
                "chat_id": chat_id,
                "error_type": "processing_error",
                "message": str(e),
                "retry_after_secs": None
            })

            raise

    async def interrupt(self, session_id: str, reason: str) -> Dict[str, Any]:
        """
        Interrupt an ongoing agent execution.

        Sets the interrupt flag for the session, which should be checked
        by the agent loop to gracefully stop processing.
        """
        logger.info(f"Interrupting session {session_id}: {reason}")

        if session_id not in self.sessions:
            raise Exception(f"Session not found: {session_id}")

        # Set interrupt flag
        if session_id in self.interrupt_flags:
            self.interrupt_flags[session_id].set()

        return {
            "status": "interrupted"
        }

    async def end_session(self, session_id: str) -> Dict[str, Any]:
        """
        End a session and clean up resources.

        Removes the session from the active sessions map and cleans up
        any associated resources.
        """
        logger.info(f"Ending session {session_id}")

        if session_id in self.sessions:
            del self.sessions[session_id]

        if session_id in self.interrupt_flags:
            del self.interrupt_flags[session_id]

        return {
            "status": "ended",
            "session_id": session_id
        }


def main():
    """Entry point for the agent bridge server."""
    # Ensure stdin/stdout are in line-buffered mode
    sys.stdin.reconfigure(line_buffering=True)
    sys.stdout.reconfigure(line_buffering=True)

    server = AgentBridgeServer()

    try:
        asyncio.run(server.run())
    except KeyboardInterrupt:
        logger.info("Shutting down agent bridge server")
    except Exception as e:
        logger.error(f"Fatal error: {e}\n{traceback.format_exc()}")
        sys.exit(1)


if __name__ == "__main__":
    main()
