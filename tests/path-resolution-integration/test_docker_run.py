#!/usr/bin/env python3
"""
Docker run path resolution integration test.

This test runs on the HOST and tests the actual `docker run` command
that users would use. It verifies that the MCP tools that require file 
path resolution work correctly in Docker mode (with CS_MOUNT_PATH set).

The tools tested are:
- code_ownership_for_path
- list_technical_debt_hotspots_for_project_file
- list_technical_debt_goals_for_project_file

Environment variables (should be set before running):
- DOCKER_IMAGE: Docker image name to test (default: codescene-mcp)
- TEST_DATA_PATH: Path to test data files (will create temp if not set)
"""

import os
import subprocess
import sys
import tempfile
import time

from mcp_test_utils import (
    MCPClient,
    ToolTestConfig,
    cleanup_test_dir,
    extract_result_text,
    print_header,
    print_test,
    print_test_summary,
)


# Forbidden pattern for Docker mode (we check for "not defined" specifically)
MOUNT_PATH_NOT_DEFINED = "CS_MOUNT_PATH"


def create_test_data() -> str:
    """Create a temporary directory with test files for Docker mounting."""
    tmpdir = tempfile.mkdtemp(prefix="mcp-docker-path-test-")
    
    test_file = os.path.join(tmpdir, "TestFile.java")
    with open(test_file, "w", encoding="utf-8") as f:
        f.write("""public class TestFile {
    public void hello() {
        System.out.println("Hello");
    }
}
""")
    
    return tmpdir


def build_docker_command(test_data_path: str, docker_image: str) -> list:
    """Build the docker run command with CS_MOUNT_PATH set."""
    return [
        'docker', 'run', '-i', '--rm',
        '-e', 'CS_MOUNT_PATH=/mount',
        '-e', 'CS_ACCESS_TOKEN=test-token',
        '-v', f'{test_data_path}:/mount:ro',
        docker_image
    ]


def run_docker_tool_test(docker_image: str, test_data_path: str, config: ToolTestConfig) -> bool:
    """
    Run a tool test in Docker mode.
    
    Similar to run_tool_test but uses Docker-specific setup.
    """
    print_header(config.header)
    
    cmd = build_docker_command(test_data_path, docker_image)
    client = MCPClient(cmd)
    
    try:
        if not client.start():
            print_test("Docker container started", False)
            return False
        print_test("Docker container started", True)
        
        client.initialize()
        
        tool_response = client.call_tool(config.tool_name, config.arguments)
        result_text = extract_result_text(tool_response)
        
        # Check for forbidden patterns - in Docker mode we check for "not defined" specifically
        has_error = any(
            pattern.lower() in result_text.lower() and "not defined" in result_text.lower()
            for pattern in config.forbidden_patterns
        )
        
        details = f"Response: {result_text[:150]}..."
        print_test(config.test_description, not has_error, details)
        return not has_error
        
    except Exception as e:
        print_test("Tool invocation", False, str(e))
        return False
    finally:
        client.stop()


def test_environment_setup(docker_image: str, test_data_path: str) -> bool:
    """Verify the test environment is correctly configured."""
    print_header("Test Environment Setup")
    
    checks = []
    
    result = subprocess.run(['docker', 'image', 'inspect', docker_image], capture_output=True)
    image_ok = result.returncode == 0
    checks.append(image_ok)
    print_test("Docker image exists", image_ok, f"Image: {docker_image}")
    
    path_ok = os.path.exists(test_data_path)
    checks.append(path_ok)
    print_test("Test data path exists", path_ok, f"Path: {test_data_path}")
    
    return all(checks)


def test_docker_run_starts(docker_image: str, test_data_path: str) -> bool:
    """Verify docker run starts the MCP server with CS_MOUNT_PATH."""
    print_header("Test Docker Run Startup (with CS_MOUNT_PATH)")
    
    cmd = build_docker_command(test_data_path, docker_image)
    print(f"  Command: docker run -i ... {docker_image}")
    
    client = MCPClient(cmd)
    try:
        started = client.start()
        print_test("Docker container started", started)
        if not started:
            stderr = client.get_stderr()
            if stderr:
                print(f"  stderr: {stderr[:200]}")
            return False
        
        # Docker containers need extra time to initialize
        time.sleep(1)
        
        response = client.initialize()
        print_test("MCP server responds to initialize", "result" in response)
        return True
    except Exception as e:
        print_test("Docker run starts", False, str(e))
        return False
    finally:
        client.stop()


def build_docker_tool_test_configs(test_data_path: str) -> list[tuple[str, ToolTestConfig]]:
    """Build all Docker tool test configurations."""
    host_file_path = os.path.join(test_data_path, "TestFile.java")
    
    return [
        ("code_ownership_for_path (Docker mode)", ToolTestConfig(
            tool_name="code_ownership_for_path",
            arguments={"project_id": 1, "path": host_file_path},
            header="Test code_ownership_for_path (Docker Mode)",
            forbidden_patterns=[MOUNT_PATH_NOT_DEFINED],
            test_description="No CS_MOUNT_PATH error",
        )),
        ("list_technical_debt_hotspots_for_project_file (Docker mode)", ToolTestConfig(
            tool_name="list_technical_debt_hotspots_for_project_file",
            arguments={"project_id": 1, "file_path": host_file_path},
            header="Test list_technical_debt_hotspots_for_project_file (Docker Mode)",
            forbidden_patterns=[MOUNT_PATH_NOT_DEFINED],
            test_description="No CS_MOUNT_PATH error",
        )),
        ("list_technical_debt_goals_for_project_file (Docker mode)", ToolTestConfig(
            tool_name="list_technical_debt_goals_for_project_file",
            arguments={"project_id": 1, "file_path": host_file_path},
            header="Test list_technical_debt_goals_for_project_file (Docker Mode)",
            forbidden_patterns=[MOUNT_PATH_NOT_DEFINED],
            test_description="No CS_MOUNT_PATH error",
        )),
    ]


def main():
    docker_image = os.getenv('DOCKER_IMAGE', 'codescene-mcp')
    test_data_path = os.getenv('TEST_DATA_PATH')
    cleanup_needed = False
    
    print("\n" + "="*60)
    print("  Docker Run Path Resolution Integration Tests")
    print("  Testing: docker run with CS_MOUNT_PATH set")
    print("="*60)
    
    if not test_data_path:
        print("\n  Creating temporary test data directory...")
        test_data_path = create_test_data()
        cleanup_needed = True
    
    print(f"  Docker image: {docker_image}")
    print(f"  Test data path: {test_data_path}")
    
    try:
        # Run setup tests
        results = [
            ("Environment Setup", test_environment_setup(docker_image, test_data_path)),
            ("Docker Run Startup", test_docker_run_starts(docker_image, test_data_path)),
        ]
        
        # Run tool tests from configuration
        for name, config in build_docker_tool_test_configs(test_data_path):
            passed = run_docker_tool_test(docker_image, test_data_path, config)
            results.append((name, passed))
        
        return print_test_summary(results)
    
    finally:
        if cleanup_needed:
            cleanup_test_dir(test_data_path)


if __name__ == '__main__':
    sys.exit(main())
