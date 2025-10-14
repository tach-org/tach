# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License.
"""
LSP session client for testing.
"""

from __future__ import annotations

import os
import subprocess
import sys
from concurrent.futures import Future, ThreadPoolExecutor
from threading import Event

from pyls_jsonrpc.dispatchers import MethodDispatcher
from pyls_jsonrpc.endpoint import Endpoint
from pyls_jsonrpc.streams import JsonRpcStreamReader, JsonRpcStreamWriter

from .constants import BUNDLED_PYTHON_LIBS_DIR
from .defaults import VSCODE_DEFAULT_INITIALIZE

# Measured in seconds
LSP_INIT_TIMEOUT = 1
LSP_EXIT_TIMEOUT = 5


PUBLISH_DIAGNOSTICS = "textDocument/publishDiagnostics"
WINDOW_LOG_MESSAGE = "window/logMessage"
WINDOW_SHOW_MESSAGE = "window/showMessage"


class LspSession(MethodDispatcher):
    """Send and Receive messages over LSP as a test LS Client."""

    def __init__(self, cwd=None):
        self.cwd = cwd if cwd else os.getcwd()

        self._thread_pool = ThreadPoolExecutor()
        self._sub = None
        self._writer = None
        self._reader = None
        self._endpoint = None
        self._notification_callbacks = {}

    def __enter__(self):
        """Context manager entrypoint.

        shell=True needed for pytest-cov to work in subprocess.
        """

        env_copy = os.environ.copy()
        env_copy["PYTHONPATH"] = str(BUNDLED_PYTHON_LIBS_DIR)
        self._sub = subprocess.Popen(
            [sys.executable, "-m", "tach", "server"],
            stdout=subprocess.PIPE,
            stdin=subprocess.PIPE,
            bufsize=0,
            cwd=self.cwd,
            env=env_copy,
            shell="WITH_COVERAGE" in os.environ,
        )
        if self._sub.stdin is not None and self._sub.stdout is not None:
            self._writer = JsonRpcStreamWriter(
                os.fdopen(self._sub.stdin.fileno(), "wb")
            )
            self._reader = JsonRpcStreamReader(
                os.fdopen(self._sub.stdout.fileno(), "rb")
            )
        else:
            raise RuntimeError("stdin and/or stdout is None")

        dispatcher = {
            PUBLISH_DIAGNOSTICS: self._publish_diagnostics,
            WINDOW_SHOW_MESSAGE: self._window_show_message,
            WINDOW_LOG_MESSAGE: self._window_log_message,
        }
        self._endpoint = Endpoint(dispatcher, self._writer.write)
        self._thread_pool.submit(self._reader.listen, self._endpoint.consume)
        self._monitoring_future = self._thread_pool.submit(self._monitor_subprocess)
        return self

    def __exit__(self, typ, value, _tb):
        if self._sub.returncode is None:  # pyright: ignore
            self.shutdown(True)
        try:
            self._sub.terminate()  # pyright: ignore
        except Exception:
            pass
        if self._endpoint is None:
            raise RuntimeError("endpoint not specified")
        self._endpoint.shutdown()
        self._thread_pool.shutdown()

    def _monitor_subprocess(self):
        self._sub.wait()  # pyright: ignore
        if self._sub.returncode != 0:  # pyright: ignore
            self.server_initialized.set()

    def initialize(
        self,
        initialize_params=None,
        process_server_capabilities=None,
    ):
        """Sends the initialize request to LSP server."""
        if initialize_params is None:
            initialize_params = VSCODE_DEFAULT_INITIALIZE
        self.server_initialized = Event()

        def _after_initialize(fut):
            if process_server_capabilities:
                process_server_capabilities(fut.result())
            self.initialized()
            self.server_initialized.set()

        self._send_request(
            "initialize",
            params=(
                initialize_params
                if initialize_params is not None
                else VSCODE_DEFAULT_INITIALIZE
            ),
            handle_response=_after_initialize,
        )

        if not self.server_initialized.wait(LSP_INIT_TIMEOUT):
            raise RuntimeError("LSP server did not initialize in time")

    def initialized(self, initialized_params=None):
        """Sends the initialized notification to LSP server."""
        if self._endpoint is None:
            raise RuntimeError("endpoint not specified")
        self._endpoint.notify("initialized", initialized_params or {})

    def shutdown(self, should_exit, exit_timeout=LSP_EXIT_TIMEOUT):
        """Sends the shutdown request to LSP server."""

        def _after_shutdown(_):
            if should_exit:
                self.exit_lsp(exit_timeout)

        self._send_request("shutdown", handle_response=_after_shutdown)

    def exit_lsp(self, exit_timeout=LSP_EXIT_TIMEOUT):
        """Handles LSP server process exit."""
        if self._endpoint is None:
            raise RuntimeError("endpoint not specified")
        self._endpoint.notify("exit")
        assert self._sub.wait(exit_timeout) == 0  # pyright: ignore

    def notify_did_change(self, did_change_params):
        """Sends did change notification to LSP Server."""
        self._send_notification("textDocument/didChange", params=did_change_params)

    def notify_did_save(self, did_save_params):
        """Sends did save notification to LSP Server."""
        self._send_notification("textDocument/didSave", params=did_save_params)

    def notify_did_open(self, did_open_params):
        """Sends did open notification to LSP Server."""
        self._send_notification("textDocument/didOpen", params=did_open_params)

    def notify_did_close(self, did_close_params):
        """Sends did close notification to LSP Server."""
        self._send_notification("textDocument/didClose", params=did_close_params)

    def text_document_formatting(self, formatting_params):
        """Sends text document references request to LSP server."""
        fut = self._send_request("textDocument/formatting", params=formatting_params)
        return fut.result()

    def text_document_code_action(self, code_action_params):
        """Sends text document code actions request to LSP server."""
        fut = self._send_request("textDocument/codeAction", params=code_action_params)
        return fut.result()

    def code_action_resolve(self, code_action_resolve_params):
        """Sends text document code actions resolve request to LSP server."""
        fut = self._send_request(
            "codeAction/resolve", params=code_action_resolve_params
        )
        return fut.result()

    def set_notification_callback(self, notification_name, callback):
        """Set custom LS notification handler."""
        self._notification_callbacks[notification_name] = callback

    def get_notification_callback(self, notification_name):
        """Gets callback if set or default callback for a given LS
        notification."""
        try:
            return self._notification_callbacks[notification_name]
        except KeyError:

            def _default_handler(_params):
                """Default notification handler."""

            return _default_handler

    def _publish_diagnostics(self, publish_diagnostics_params):
        """Internal handler for text document publish diagnostics."""
        return self._handle_notification(
            PUBLISH_DIAGNOSTICS, publish_diagnostics_params
        )

    def _window_log_message(self, window_log_message_params):
        """Internal handler for window log message."""
        return self._handle_notification(WINDOW_LOG_MESSAGE, window_log_message_params)

    def _window_show_message(self, window_show_message_params):
        """Internal handler for window show message."""
        return self._handle_notification(
            WINDOW_SHOW_MESSAGE, window_show_message_params
        )

    def _handle_notification(self, notification_name, params):
        """Internal handler for notifications."""
        fut = Future()

        def _handler():
            callback = self.get_notification_callback(notification_name)
            callback(params)
            fut.set_result(None)

        self._thread_pool.submit(_handler)
        return fut

    def _send_request(self, name, params=None, handle_response=lambda f: f.done()):
        """Sends {name} request to the LSP server."""
        if self._endpoint is None:
            raise RuntimeError("endpoint not specified")
        fut = self._endpoint.request(name, params)
        fut.add_done_callback(handle_response)
        return fut

    def _send_notification(self, name, params=None):
        """Sends {name} notification to the LSP server."""
        if self._endpoint is None:
            raise RuntimeError("endpoint not specified")
        self._endpoint.notify(name, params)
