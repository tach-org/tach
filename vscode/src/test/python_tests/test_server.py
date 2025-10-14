# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License.
"""
Test for linting over LSP.
"""

from __future__ import annotations

from threading import Event

import pytest
from hamcrest import assert_that, is_

from .lsp_test_client import constants, defaults, session, utils

SERVER_INFO = utils.get_server_info_defaults()
TIMEOUT = 2  # 2 seconds


@pytest.mark.parametrize(
    "test_file_path, expected",
    [
        (
            constants.TEST_DATA / "sample1" / "sample.py",
            {
                "uri": utils.as_uri(str(constants.TEST_DATA / "sample1" / "sample.py")),
                "diagnostics": [
                    {
                        "range": {
                            "start": {"line": 2, "character": 0},
                            "end": {"line": 2, "character": 99999},
                        },
                        "message": "Cannot use 'sample2.sample2.SAMPLE2'. Module 'sample1' cannot depend on 'sample2'.",
                        "severity": 1,
                        "source": "tach",
                    }
                ],
            },
        ),
        (
            constants.TEST_DATA / "sample2" / "sample2.py",
            {
                "uri": utils.as_uri(
                    str(constants.TEST_DATA / "sample2" / "sample2.py")
                ),
                "diagnostics": [
                    {
                        "message": "The path 'sample1.sample.SAMPLE1' is not part of the public interface for 'sample1'.",
                        "range": {
                            "start": {"line": 2, "character": 0},
                            "end": {"line": 2, "character": 99999},
                        },
                        "severity": 1,
                        "source": "tach",
                    }
                ],
            },
        ),
    ],
)
def test_import_example(test_file_path, expected):
    """Test to linting on file open."""
    test_file_uri = utils.as_uri(str(test_file_path))

    contents = test_file_path.read_text()

    actual = []
    with session.LspSession(cwd=constants.TEST_DATA) as ls_session:
        ls_session.initialize(defaults.VSCODE_DEFAULT_INITIALIZE)

        done = Event()

        def _handler(params):
            nonlocal actual
            actual = params
            done.set()

        ls_session.set_notification_callback(session.PUBLISH_DIAGNOSTICS, _handler)

        ls_session.notify_did_open(
            {
                "textDocument": {
                    "uri": test_file_uri,
                    "languageId": "python",
                    "version": 1,
                    "text": contents,
                }
            }
        )
        # wait for some time to receive all notifications
        done.wait(TIMEOUT)

    assert_that(actual, is_(expected))
