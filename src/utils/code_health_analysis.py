import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from errors import CodeSceneCliError
from .docker_path_adapter import adapt_mounted_file_path_inside_docker
from .platform_details import get_platform_details, get_ssl_cli_args
from .docker_path_adapter import get_worktree_gitdir

def find_git_root(file_path: str) -> str:
    """
    Finds the git repository root for a given file path.
    
    Args:
        file_path (str): Path to a file in the repository
        
    Returns:
        str: Path to the git repository root
        
    Raises:
        CodeSceneCliError: If not in a git repository
    """
    current_path = Path(file_path).resolve()
    
    # If it's a file, start from its parent directory
    if current_path.is_file():
        current_path = current_path.parent
    
    # Walk up the directory tree looking for .git
    while current_path != current_path.parent:
        if (current_path / '.git').exists():
            return str(current_path)
        current_path = current_path.parent
    
    raise CodeSceneCliError(f"Not in a git repository: {file_path}")


def run_local_tool(command: list, cwd: str | None = None, extra_env: dict | None = None):
    """
    Runs a local command-line tool and captures its output.

    Args:
        command (list): The command and its arguments, e.g. ['ls', '-l']
        cwd (str): Optional working directory to run the command in
        extra_env (dict): Optional extra environment variables to set

    Returns:
        str: Combined stdout and stderr output
    """
    # Start with a copy of the current environment
    env = os.environ.copy()
    
    # Override/add MCP-specific variables
    env['CS_CONTEXT'] = 'mcp-server'
    env['CS_ACCESS_TOKEN'] = os.getenv("CS_ACCESS_TOKEN", "")

    if os.getenv("CS_ONPREM_URL"):
        env['CS_ONPREM_URL'] = os.getenv("CS_ONPREM_URL")

    # Apply platform-specific environment configuration
    platform = get_platform_details()
    env = platform.configure_environment(env)

    # Apply any extra environment variables (e.g., GIT_DIR for worktrees)
    if extra_env:
        env.update(extra_env)

    # Check if this is a CS CLI command and inject SSL args if needed
    # SSL args must be passed directly to the CLI (GraalVM native image doesn't read _JAVA_OPTIONS)
    actual_command = command
    if command and _is_cs_cli_command(command[0]):
        ssl_args = get_ssl_cli_args()
        if ssl_args:
            # Insert SSL args after the CLI binary, before subcommand
            actual_command = [command[0]] + ssl_args + command[1:]

    # Use 'replace' error handling to gracefully handle encoding issues on Windows
    # where the CLI may output text in CP1252 instead of UTF-8
    result = subprocess.run(actual_command, capture_output=True, text=True, encoding="utf-8", errors="replace", cwd=cwd, env=env)
    if result.returncode != 0:
        raise CodeSceneCliError(f"CLI command failed: {result.stderr}")
    
    return result.stdout


def _is_cs_cli_command(cmd: str) -> bool:
    """Check if the command is a CS CLI binary."""
    if not cmd:
        return False
    # Check if command ends with 'cs' or 'cs.exe' (the CLI binary names)
    cmd_lower = cmd.lower()
    # Normalize path separators and get the basename
    basename = cmd_lower.replace('\\', '/').split('/')[-1]
    return basename in ('cs', 'cs.exe')


def run_cs_cli(cli_fn) -> str:
    """
    Encapsulates the general pattern of invoking the CLI tool and
    propagating potential errors.
    """
    try:
        return cli_fn()
    except FileNotFoundError:
        return "Error: The CodeScene CLI tool, cs, isn't properly installed. See https://codescene.io/docs/cli/index.html for instructions."
    except subprocess.CalledProcessError as e:
        return f"Error: {e.stderr}"
    except Exception as e:
        return f"Error: {e}"


def code_health_from_cli_output(cli_output) -> float:
    r = json.loads(cli_output)

    if 'score' not in r:
        raise CodeSceneCliError("CLI output does not contain a 'score' field: {}".format(cli_output))

    return r['score']


def _ensure_executable(path: Path) -> str:
    """Ensure the path is executable and return it as a string."""
    if not os.access(path, os.X_OK):
        os.chmod(path, 0o755)
    return str(path)


def _try_nuitka_cli_path(cli_binary_name: str):
    """
    Try to find CLI binary in Nuitka compiled environment.
    
    For onefile builds with --include-data-files, the cs binary location differs:
    - Linux: next to the executable (sys.executable.parent)
    - Windows: in the extraction root (traverse up from __file__ until we find it)
    
    Returns:
        str: Path to CLI binary if found, None otherwise.
    """
    try:
        # Check if we're running in a Nuitka compiled environment
        __compiled__  # type: ignore[name-defined]
        
        import sys
        
        print(f"DEBUG Nuitka: sys.executable={sys.executable}", file=sys.stderr)
        print(f"DEBUG Nuitka: __file__={__file__}", file=sys.stderr)
        print(f"DEBUG Nuitka: sys.argv[0]={sys.argv[0]}", file=sys.stderr)
        print(f"DEBUG Nuitka: Looking for {cli_binary_name}", file=sys.stderr)
        
        # Try locations in order of likelihood
        locations_to_try = [
            # Linux: next to the executable
            Path(sys.executable).parent / cli_binary_name,
            # Alternative: relative to sys.argv[0]
            Path(sys.argv[0]).parent.absolute() / cli_binary_name,
        ]
        
        # Windows: Walk up from __file__ to find the cs binary
        # On Windows, __file__ might be something like:
        # C:/Users/.../Temp/onefile_12345/.../src/utils/code_health_analysis.py
        # and cs.exe would be in the onefile_12345 directory
        current = Path(__file__).parent
        for _ in range(10):  # Try up to 10 levels up
            candidate = current / cli_binary_name
            locations_to_try.append(candidate)
            print(f"DEBUG Nuitka: Checking parent level {current}", file=sys.stderr)
            if current.parent == current:  # Reached root
                break
            current = current.parent
        
        for location in locations_to_try:
            print(f"DEBUG Nuitka: Checking {location}", file=sys.stderr)
            if location.exists():
                print(f"DEBUG Nuitka: Found cs binary at {location}", file=sys.stderr)
                return _ensure_executable(location)
        
        print(f"DEBUG Nuitka: cs binary not found in any location", file=sys.stderr)
            
    except NameError:
        import sys
        print(f"DEBUG: Not in Nuitka environment (__compiled__ not found)", file=sys.stderr)
    return None


def _try_bundled_cli_path(cli_binary_name: str):
    """
    Try to find CLI binary in source tree (development mode).
    
    Returns:
        str: Path to CLI binary if found, None otherwise.
    """
    bundle_dir = Path(__file__).parent.parent.parent.absolute()
    internal_cs_path = bundle_dir / cli_binary_name
    if internal_cs_path.exists():
        return _ensure_executable(internal_cs_path)
    return None


DOCKER_CLI_PATH = '/root/.local/bin/cs'


def cs_cli_path(platform_details):
    """
    Determine the path to the CodeScene CLI binary.
    
    Resolution order:
    1. CS_CLI_PATH environment variable override
    2. Docker environment static path
    3. Nuitka compiled binary location
    4. Bundled binary in source tree (development mode)
    5. Default Docker path (fallback)
    """
    env_override = os.getenv("CS_CLI_PATH")
    if env_override:
        return env_override

    if os.getenv("CS_MOUNT_PATH"):
        return DOCKER_CLI_PATH

    cli_binary_name = platform_details.get_cli_binary_name()

    nuitka_path = _try_nuitka_cli_path(cli_binary_name)
    if nuitka_path:
        return nuitka_path

    bundled_path = _try_bundled_cli_path(cli_binary_name)
    if bundled_path:
        return bundled_path

    return DOCKER_CLI_PATH


def make_cs_cli_review_command_for(cli_command: str, file_path: str, platform_details=None):
    if platform_details is None:
        platform_details = get_platform_details()
    cs_cli = cs_cli_path(platform_details)

    if os.getenv("CS_MOUNT_PATH"):
        mount_file_path = adapt_mounted_file_path_inside_docker(file_path)
    else:
        mount_file_path = file_path

    return [cs_cli, cli_command, mount_file_path, "--output-format=json"]


def cs_cli_review_command_for(file_path: str, platform_details=None):
    return make_cs_cli_review_command_for("review", file_path, platform_details)


def analyze_code(file_path: str) -> str:
    if os.getenv("CS_MOUNT_PATH"):
        # Docker environment - use file path directly, path adaptation handled by cs_cli_review_command_for
        return run_local_tool(cs_cli_review_command_for(file_path))
    else:
        # Local/Nuitka binary - find git root and use relative path
        git_root = find_git_root(file_path)
        relative_path = str(Path(file_path).relative_to(git_root))
        
        # Detect worktree and set GIT_DIR if needed (mirrors Docker mode logic)
        gitdir = get_worktree_gitdir(git_root)
        extra_env = {"GIT_DIR": gitdir} if gitdir else None
        
        return run_local_tool(cs_cli_review_command_for(relative_path), cwd=git_root, extra_env=extra_env)
