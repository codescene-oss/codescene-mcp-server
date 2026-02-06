#!/usr/bin/env python3
"""
Shared utilities for comprehensive MCP integration tests.

This module re-exports all test utilities for backward compatibility.
The actual implementations are in:
- test_output.py: Print utilities (print_header, print_test, print_summary)
- mcp_client.py: MCPClient for communicating with MCP servers
- response_parsers.py: extract_result_text, extract_code_health_score
- server_backends.py: ServerBackend, NuitkaBackend, DockerBackend, BuildConfig, ExecutableBuilder
- file_utils.py: create_test_environment, create_git_repo, cleanup_dir, safe_temp_directory
"""

# Re-export from test_output
# Re-export from file_utils
from file_utils import (
    cleanup_dir,
    create_git_repo,
    create_test_environment,
    safe_temp_directory,
)

# Re-export from mcp_client
from mcp_client import (
    MCPClient,
    next_msg_id,
)

# Re-export from response_parsers
from response_parsers import (
    extract_code_health_score,
    extract_result_text,
)

# Re-export from server_backends
from server_backends import (
    BuildConfig,
    DockerBackend,
    ExecutableBuilder,
    NuitkaBackend,
    ServerBackend,
)
from test_output import (
    print_header,
    print_summary,
    print_test,
)

# Export all public names
__all__ = [
    # test_output
    "print_header",
    "print_test",
    "print_summary",
    # mcp_client
    "MCPClient",
    "next_msg_id",
    # response_parsers
    "extract_result_text",
    "extract_code_health_score",
    # server_backends
    "ServerBackend",
    "BuildConfig",
    "ExecutableBuilder",
    "NuitkaBackend",
    "DockerBackend",
    # file_utils
    "create_test_environment",
    "create_git_repo",
    "cleanup_dir",
    "safe_temp_directory",
]
