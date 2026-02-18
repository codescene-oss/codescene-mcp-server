#!/usr/bin/env python3
"""
Server backend abstractions for integration tests.

This module provides:
- ServerBackend protocol for abstracting server execution
- NuitkaBackend for running compiled executables
- DockerBackend for running in containers
- BuildConfig and ExecutableBuilder for building executables
"""

import os
import platform
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile
from abc import ABC, abstractmethod
from dataclasses import dataclass
from pathlib import Path

from test_output import print_header


class ServerBackend(ABC):
    """Abstract backend for running the MCP server."""

    @abstractmethod
    def prepare(self) -> None:
        """Prepare the backend (build executable, build image, etc.)."""
        pass

    @abstractmethod
    def get_command(self, working_dir: Path) -> list[str]:
        """Get the command to launch the MCP server."""
        pass

    @abstractmethod
    def get_env(self, base_env: dict[str, str], working_dir: Path) -> dict[str, str]:
        """Get environment variables for the server process."""
        pass

    @abstractmethod
    def cleanup(self) -> None:
        """Clean up any resources."""
        pass


@dataclass
class BuildConfig:
    """Configuration for building the static executable."""

    repo_root: Path
    build_dir: Path
    python_executable: str = "python3.13"
    version: str = "MCP-0.0.0-test"


class ExecutableBuilder:
    """Handles building the static executable in an isolated environment."""

    # Platform-specific CLI download URLs
    CLI_URLS = {
        (
            "Darwin",
            "arm64",
        ): "https://downloads.codescene.io/enterprise/cli/cs-macos-aarch64-latest.zip",
        (
            "Darwin",
            "aarch64",
        ): "https://downloads.codescene.io/enterprise/cli/cs-macos-aarch64-latest.zip",
        (
            "Darwin",
            "x86_64",
        ): "https://downloads.codescene.io/enterprise/cli/cs-macos-amd64-latest.zip",
        (
            "Darwin",
            "amd64",
        ): "https://downloads.codescene.io/enterprise/cli/cs-macos-amd64-latest.zip",
        (
            "Linux",
            "aarch64",
        ): "https://downloads.codescene.io/enterprise/cli/cs-linux-aarch64-latest.zip",
        (
            "Linux",
            "arm64",
        ): "https://downloads.codescene.io/enterprise/cli/cs-linux-aarch64-latest.zip",
        (
            "Linux",
            "x86_64",
        ): "https://downloads.codescene.io/enterprise/cli/cs-linux-amd64-latest.zip",
        (
            "Linux",
            "amd64",
        ): "https://downloads.codescene.io/enterprise/cli/cs-linux-amd64-latest.zip",
        (
            "Windows",
            "amd64",
        ): "https://downloads.codescene.io/enterprise/cli/cs-windows-amd64-latest.zip",
        (
            "Windows",
            "x86_64",
        ): "https://downloads.codescene.io/enterprise/cli/cs-windows-amd64-latest.zip",
    }

    def __init__(self, config: BuildConfig):
        self.config = config

    def _get_cli_download_url(self) -> str:
        """Get the appropriate CLI download URL for the current platform."""
        system = platform.system()
        machine = platform.machine().lower()

        url = self.CLI_URLS.get((system, machine))
        if url:
            return url

        # Fallback for unrecognized architectures
        fallback_key = (system, "x86_64")
        url = self.CLI_URLS.get(fallback_key)
        if url:
            return url

        raise RuntimeError(f"Unsupported platform: {system} {machine}")

    def _download_cli(self, dest_dir: Path) -> Path:
        """
        Download the CodeScene CLI for the current platform.

        Args:
            dest_dir: Directory to download and extract CLI to

        Returns:
            Path to the extracted CLI executable
        """
        url = self._get_cli_download_url()
        print(f"  Downloading CLI from: {url}")

        zip_path = dest_dir / "cli.zip"
        urllib.request.urlretrieve(url, zip_path)

        # Extract the zip
        with zipfile.ZipFile(zip_path, "r") as zip_ref:
            zip_ref.extractall(dest_dir)

        # Find the CLI executable
        is_windows = os.name == "nt" or platform.system() == "Windows"
        cli_name = "cs.exe" if is_windows else "cs"

        cli_path = dest_dir / cli_name
        if not cli_path.exists():
            # Try to find it in subdirectories
            for file_path in dest_dir.rglob(cli_name):
                cli_path = file_path
                break

        if not cli_path.exists():
            raise FileNotFoundError(f"Could not find {cli_name} after extraction")

        # Make executable on Unix-like systems
        if not is_windows:
            cli_path.chmod(0o755)

        print(f"  Downloaded CLI to: {cli_path}")
        return cli_path

    def _is_windows(self) -> bool:
        """Check if running on Windows."""
        return os.name == "nt" or platform.system() == "Windows"

    def _get_cli_name(self) -> str:
        """Get the platform-specific CLI executable name."""
        return "cs.exe" if self._is_windows() else "cs"

    def _get_executable_name(self) -> str:
        """Get the platform-specific output executable name."""
        return "cs-mcp.exe" if self._is_windows() else "cs-mcp"

    def _copy_source_files(self) -> None:
        """Copy source files to the build directory."""
        print("  Copying source files to build directory...")
        src_dest = self.config.build_dir / "src"
        if src_dest.exists():
            shutil.rmtree(src_dest)
        shutil.copytree(self.config.repo_root / "src", src_dest)

        docs_dest = self.config.build_dir / "src" / "docs"
        if docs_dest.exists():
            shutil.rmtree(docs_dest)
        shutil.copytree(self.config.repo_root / "src" / "docs", docs_dest)

    def _inject_test_version(self) -> None:
        """Replace __version__ = "dev" with a test version in the build copy.

        This mirrors the version injection done by the CI workflow (sed in
        build-exe.yml) so that the test binary behaves like a release build —
        in particular, the version checker will perform real background fetches
        instead of short-circuiting on the "dev" sentinel.
        """
        version_file = self.config.build_dir / "src" / "version.py"
        content = version_file.read_text()
        new_content = content.replace('__version__ = "dev"', f'__version__ = "{self.config.version}"')
        version_file.write_text(new_content)
        print(f"  Injected test version: {self.config.version}")

    def _ensure_cli_available(self) -> None:
        """Ensure the CodeScene CLI is available in the build directory."""
        cs_name = self._get_cli_name()
        cs_source = self.config.repo_root / cs_name
        cs_dest = self.config.build_dir / cs_name

        if cs_source.exists():
            shutil.copy2(cs_source, cs_dest)
            print(f"  Copied {cs_name} from repo root")
            return

        print(f"  No {cs_name} found in repo root, downloading...")
        cli_path = self._download_cli(self.config.build_dir)
        if cli_path != cs_dest:
            shutil.move(str(cli_path), str(cs_dest))

    def _run_nuitka_build(self) -> Path:
        """Run Nuitka to build the executable."""
        print("  Building with Nuitka (this may take several minutes)...")
        sys.stdout.flush()

        executable_name = self._get_executable_name()
        cs_data_file = self._get_cli_name()

        build_cmd = [
            self.config.python_executable,
            "-m",
            "nuitka",
            "--onefile",
            "--assume-yes-for-downloads",
            "--show-progress",
            "--include-data-dir=./src/docs=src/docs",
            "--include-data-dir=./src/code_health_refactoring_business_case/s_curve/regression=code_health_refactoring_business_case/s_curve/regression",
            f"--include-data-files=./{cs_data_file}={cs_data_file}",
            f"--output-filename={executable_name}",
            "src/cs_mcp_server.py",
        ]

        # Run with output visible to avoid GitHub Actions timeout
        result = subprocess.run(build_cmd, cwd=str(self.config.build_dir), text=True)

        if result.returncode != 0:
            print(f"  \033[91mBuild failed with exit code {result.returncode}\033[0m")
            raise RuntimeError("Nuitka build failed")

        binary_path = self.config.build_dir / executable_name
        if not binary_path.exists():
            raise FileNotFoundError(f"Binary not found at {binary_path} after build")

        return binary_path

    def build(self) -> Path:
        """
        Build the static executable using Nuitka.

        Returns:
            Path to the built executable in the isolated build directory
        """
        print_header("Building Static Executable")

        self.config.build_dir.mkdir(parents=True, exist_ok=True)
        self._copy_source_files()
        self._inject_test_version()
        self._ensure_cli_available()

        binary_path = self._run_nuitka_build()
        print(f"  \033[92mBuild successful:\033[0m {binary_path}")
        return binary_path


class NuitkaBackend(ServerBackend):
    """Backend that uses a Nuitka-compiled executable."""

    def __init__(self, executable: Path | None = None):
        self.executable = executable
        self._temp_dir: Path | None = None

    def prepare(self) -> None:
        """Build the Nuitka executable if not already provided."""
        if self.executable:
            print(f"\nUsing existing executable: {self.executable}")
            return

        repo_root = Path(__file__).parent.parent.parent
        self._temp_dir = Path(tempfile.mkdtemp(prefix="cs_mcp_build_"))
        build_dir = self._temp_dir / "build"

        config = BuildConfig(repo_root=repo_root, build_dir=build_dir, python_executable=sys.executable)

        builder = ExecutableBuilder(config)
        binary_path = builder.build()

        # Move to persistent location outside repo
        test_bin_dir = repo_root.parent / "cs_mcp_test_bin"
        test_bin_dir.mkdir(exist_ok=True)
        self.executable = test_bin_dir / binary_path.name
        shutil.copy2(binary_path, self.executable)

        if os.name != "nt":
            os.chmod(self.executable, 0o755)

        print(f"\n\033[92mExecutable ready:\033[0m {self.executable}")

    def get_command(self, working_dir: Path) -> list[str]:
        """Return command to run the Nuitka executable."""
        return [str(self.executable)]

    def get_env(self, base_env: dict[str, str], working_dir: Path) -> dict[str, str]:
        """Return environment without CS_MOUNT_PATH for native execution."""
        env = base_env.copy()
        env.pop("CS_MOUNT_PATH", None)
        # Disable the version check by default so non-version-check tests
        # don't make real HTTP calls to GitHub and don't get the "VERSION
        # UPDATE AVAILABLE" banner prepended to tool responses.
        # test_version_check.py overrides this after calling get_env().
        env.setdefault("CS_DISABLE_VERSION_CHECK", "1")
        return env

    def cleanup(self) -> None:
        """Clean up temporary build directory."""
        if self._temp_dir and self._temp_dir.exists():
            shutil.rmtree(self._temp_dir, ignore_errors=True)


class DockerBackend(ServerBackend):
    """Backend that uses Docker to run the MCP server.

    The Docker setup works as follows:
    - CS_MOUNT_PATH is set to the HOST path (e.g., /tmp/test_repo)
    - The mount destination is /mount/ inside the container
    - The server translates paths from CS_MOUNT_PATH to /mount/ internally
    - Tests pass host paths, and the server handles the translation
    """

    IMAGE_NAME = "codescene-mcp-test"
    CONTAINER_MOUNT_DEST = "/mount/"
    TEST_VERSION = "MCP-0.0.0-test"

    def __init__(self, image_name: str | None = None):
        self.image_name = image_name or self.IMAGE_NAME
        self._built = False

    def prepare(self) -> None:
        """Build the Docker image with a non-dev version injected.

        The Dockerfile accepts a VERSION build arg that replaces the default
        "dev" sentinel in version.py.  Without this, the version checker
        short-circuits on "dev" and never performs background fetches, which
        would cause the version-check integration tests to fail.
        """
        print_header("Building Docker Image")
        repo_root = Path(__file__).parent.parent.parent

        result = subprocess.run(
            [
                "docker",
                "build",
                "--build-arg",
                f"VERSION={self.TEST_VERSION}",
                "-t",
                self.image_name,
                ".",
            ],
            cwd=str(repo_root),
            text=True,
        )

        if result.returncode != 0:
            raise RuntimeError("Docker build failed")

        self._built = True
        print(f"\n\033[92mDocker image ready:\033[0m {self.image_name}")

    def get_command(self, working_dir: Path) -> list[str]:
        """Return docker run command with proper mounts.

        Following the documented pattern:
        - CS_MOUNT_PATH is set to the HOST path (working_dir)
        - Mount binds host path to /mount/ in container
        - Server translates paths internally
        """
        return [
            "docker",
            "run",
            "-i",
            "--rm",
            "-e",
            "CS_ACCESS_TOKEN",
            "-e",
            f"CS_MOUNT_PATH={working_dir}",
            "-e",
            "CS_ONPREM_URL",
            "-e",
            "CS_VERSION_CHECK_URL",
            "-e",
            "CS_DISABLE_VERSION_CHECK",
            "-e",
            "CS_TRACKING_URL",
            "-e",
            "CS_DISABLE_TRACKING",
            "--add-host=host.docker.internal:host-gateway",
            "--mount",
            f"type=bind,src={working_dir},dst={self.CONTAINER_MOUNT_DEST}",
            self.image_name,
        ]

    def get_env(self, base_env: dict[str, str], working_dir: Path) -> dict[str, str]:
        """Return environment for Docker execution."""
        env = base_env.copy()
        # Disable the version check by default so non-version-check tests
        # don't make real HTTP calls to GitHub and don't get the "VERSION
        # UPDATE AVAILABLE" banner prepended to tool responses.
        # test_version_check.py overrides this after calling get_env().
        env.setdefault("CS_DISABLE_VERSION_CHECK", "1")
        return env

    def cleanup(self) -> None:
        """No cleanup needed - containers use --rm."""
        pass
