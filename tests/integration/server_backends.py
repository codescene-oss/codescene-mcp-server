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


class NpmBackend(ServerBackend):
    """Backend that tests the full npm package installation and binary download flow.

    Simulates real-world user experience:
    1. Builds the Nuitka binary (or uses a pre-built one)
    2. Packs the npm package with `npm pack`
    3. Installs the tarball into a temp directory with `npm install`
    4. Starts a local HTTP server that serves the binary as a zip,
       matching the GitHub releases URL pattern (/{tag}/{asset})
    5. Invokes the package via `npx @codescene/codehealth-mcp` with
       CS_MCP_DOWNLOAD_BASE_URL pointing at the local server

    This exercises the download, extraction, caching, and launch pipeline
    end-to-end, without hitting GitHub.
    """

    def __init__(self, executable: Path | None = None):
        self._nuitka_backend = NuitkaBackend(executable=executable)
        self._repo_root = Path(__file__).parent.parent.parent
        self._install_dir: Path | None = None
        self._http_server: subprocess.Popen | None = None
        self._http_port: int | None = None
        self._serve_dir: Path | None = None

    def _find_node(self) -> str:
        """Find the Node.js binary on PATH."""
        node_path = shutil.which("node")
        if not node_path:
            raise RuntimeError(
                "Node.js not found in PATH. "
                "Install Node.js >= 18 to run npm backend tests."
            )
        return node_path

    def _find_npm(self) -> str:
        """Find the npm binary on PATH."""
        npm_path = shutil.which("npm")
        if not npm_path:
            raise RuntimeError("npm not found in PATH.")
        return npm_path

    def _find_npx(self) -> str:
        """Find the npx binary on PATH."""
        npx_path = shutil.which("npx")
        if not npx_path:
            raise RuntimeError("npx not found in PATH.")
        return npx_path

    def _read_package_version(self) -> str:
        """Read the version from npm/package.json."""
        import json

        pkg_path = self._repo_root / "npm" / "package.json"
        with open(pkg_path) as f:
            return json.load(f)["version"]

    # Maps platform.system() to (os_label, is_zipped)
    _PLATFORM_OS_MAP: dict[str, tuple[str, bool]] = {
        "Darwin": ("macos", True),
        "Linux": ("linux", True),
        "Windows": ("windows", False),
    }

    def _get_platform_asset_info(self) -> tuple[str, str]:
        """Return (asset_filename, binary_name_inside_zip) for the current platform.

        Mirrors the naming convention from the GitHub release pipeline.
        """
        system = platform.system()
        machine = platform.machine().lower()

        entry = self._PLATFORM_OS_MAP.get(system)
        if entry is None:
            raise RuntimeError(f"Unsupported platform: {system} {machine}")

        os_label, is_zipped = entry
        arch = "aarch64" if machine in ("arm64", "aarch64") else "amd64"
        binary_name = f"cs-mcp-{os_label}-{arch}"

        if is_zipped:
            return f"{binary_name}.zip", binary_name
        return f"{binary_name}.exe", f"{binary_name}.exe"

    def _pack_npm_package(self) -> Path:
        """Run `npm pack` in the npm/ directory and return the tarball path."""
        npm = self._find_npm()
        npm_dir = self._repo_root / "npm"

        result = subprocess.run(
            [npm, "pack", "--pack-destination", str(npm_dir)],
            cwd=str(npm_dir),
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            raise RuntimeError(f"npm pack failed:\n{result.stderr}")

        # npm pack prints the tarball filename to stdout
        tarball_name = result.stdout.strip().splitlines()[-1]
        tarball_path = npm_dir / tarball_name
        if not tarball_path.exists():
            raise FileNotFoundError(f"Expected tarball not found: {tarball_path}")

        print(f"  Packed: {tarball_path}")
        return tarball_path

    def _install_tarball(self, tarball: Path) -> Path:
        """Install the tarball into an isolated directory, return the install dir."""
        npm = self._find_npm()
        install_dir = Path(tempfile.mkdtemp(prefix="cs_mcp_npm_install_"))

        # Create a minimal package.json so npm install works
        import json

        init_pkg = {"name": "npm-backend-test", "version": "0.0.0", "private": True}
        (install_dir / "package.json").write_text(json.dumps(init_pkg))

        result = subprocess.run(
            [npm, "install", str(tarball)],
            cwd=str(install_dir),
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            raise RuntimeError(f"npm install failed:\n{result.stderr}")

        print(f"  Installed into: {install_dir}")
        return install_dir

    def _prepare_serve_directory(self, binary_path: Path) -> Path:
        """Create a directory structure matching GitHub release URLs and return it.

        Produces: {serve_dir}/MCP-{version}/{asset}
        On macOS/Linux the asset is a zip containing the platform-named binary.
        On Windows the asset is the bare .exe.
        """
        version = self._read_package_version()
        tag = f"MCP-{version}"
        asset_name, inner_binary_name = self._get_platform_asset_info()

        serve_dir = Path(tempfile.mkdtemp(prefix="cs_mcp_npm_serve_"))
        tag_dir = serve_dir / tag
        tag_dir.mkdir()

        if asset_name.endswith(".zip"):
            # Create a zip containing the binary with the platform-specific name
            zip_path = tag_dir / asset_name
            with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
                zf.write(binary_path, inner_binary_name)
            print(f"  Prepared zip: {zip_path} (contains {inner_binary_name})")
        else:
            # Windows: serve the bare exe
            shutil.copy2(binary_path, tag_dir / asset_name)
            print(f"  Prepared binary: {tag_dir / asset_name}")

        return serve_dir

    def _start_http_server(self, serve_dir: Path) -> int:
        """Start a Python HTTP server serving serve_dir, return the port."""
        # Use port 0 to let the OS assign a free port, then read it back
        self._http_server = subprocess.Popen(
            [
                sys.executable,
                "-c",
                (
                    "import http.server, socketserver, sys, os\n"
                    "os.chdir(sys.argv[1])\n"
                    "handler = http.server.SimpleHTTPRequestHandler\n"
                    "with socketserver.TCPServer(('127.0.0.1', 0), handler) as s:\n"
                    "    port = s.server_address[1]\n"
                    "    print(port, flush=True)\n"
                    "    s.serve_forever()\n"
                ),
                str(serve_dir),
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        # Read the port from the server's stdout
        assert self._http_server.stdout is not None
        port_line = self._http_server.stdout.readline().strip()
        if not port_line.isdigit():
            assert self._http_server.stderr is not None
            stderr = self._http_server.stderr.read()
            raise RuntimeError(f"Failed to start HTTP server: {stderr}")

        port = int(port_line)
        print(f"  HTTP server listening on 127.0.0.1:{port}")
        return port

    def prepare(self) -> None:
        """Build binary, pack npm package, install it, and start the file server."""
        print_header("Preparing npm Backend")

        # 1. Build the Nuitka binary
        self._nuitka_backend.prepare()
        binary_path = self._nuitka_backend.executable
        assert binary_path is not None, "Nuitka backend did not produce a binary"

        # 2. Verify Node.js
        node = self._find_node()
        print(f"\n  Node.js: {node}")

        # 3. Pack the npm package
        tarball = self._pack_npm_package()

        try:
            # 4. Install into isolated directory
            self._install_dir = self._install_tarball(tarball)

            # 5. Prepare the serve directory with the zipped binary
            self._serve_dir = self._prepare_serve_directory(binary_path)

            # 6. Start the local HTTP server
            self._http_port = self._start_http_server(self._serve_dir)
        finally:
            # Clean up the tarball
            tarball.unlink(missing_ok=True)

        # Verify the bin entry point exists in the installed package
        entry_point = (
            self._install_dir
            / "node_modules"
            / "@codescene"
            / "codehealth-mcp"
            / "bin"
            / "cs-mcp.js"
        )
        if not entry_point.exists():
            raise FileNotFoundError(
                f"Installed bin entry point not found: {entry_point}"
            )
        print(f"  Entry point: {entry_point}")
        print(f"\n\033[92mnpm backend ready\033[0m")

    def get_command(self, working_dir: Path) -> list[str]:
        """Return command to invoke the package via npx, matching real user experience."""
        assert self._install_dir is not None, "prepare() must be called first"
        npx = self._find_npx()
        return [
            npx, "--yes", "--prefix", str(self._install_dir),
            "@codescene/codehealth-mcp",
        ]

    def get_env(self, base_env: dict[str, str], working_dir: Path) -> dict[str, str]:
        """Return environment pointing at the local download server."""
        env = base_env.copy()
        env.pop("CS_MOUNT_PATH", None)
        # Point the npm wrapper at our local HTTP server instead of GitHub
        env["CS_MCP_DOWNLOAD_BASE_URL"] = (
            f"http://127.0.0.1:{self._http_port}"
        )
        # Remove any CS_MCP_BINARY_PATH so the wrapper goes through the download path
        env.pop("CS_MCP_BINARY_PATH", None)
        # Disable version check by default
        env.setdefault("CS_DISABLE_VERSION_CHECK", "1")
        return env

    def cleanup(self) -> None:
        """Stop the HTTP server and clean up temp directories."""
        if self._http_server:
            self._http_server.terminate()
            try:
                self._http_server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self._http_server.kill()
            self._http_server = None

        if self._install_dir and self._install_dir.exists():
            shutil.rmtree(self._install_dir, ignore_errors=True)

        if self._serve_dir and self._serve_dir.exists():
            shutil.rmtree(self._serve_dir, ignore_errors=True)

        self._nuitka_backend.cleanup()
