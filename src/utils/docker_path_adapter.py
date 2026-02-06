import os
from pathlib import Path, PureWindowsPath
from errors import CodeSceneCliError


def get_relative_path_from_git_root(file_path: str, git_root: str) -> str:
    """
    Get the relative path of a file from the git root, handling path normalization.
    
    Both paths are resolved to handle:
    - Relative vs absolute paths
    - Symlinks
    - Windows path quirks (case sensitivity, drive letters)
    
    This function fixes the "not in subpath" error that occurs when file_path
    is relative but git_root is absolute (resolved), causing Path.relative_to()
    to fail.
    
    Args:
        file_path: Path to the file (can be relative or absolute)
        git_root: Path to the git repository root
        
    Returns:
        Relative path string from git_root to file_path
        
    Raises:
        CodeSceneCliError: If file_path is not under git_root
    """
    resolved_file = Path(file_path).resolve()
    resolved_root = Path(git_root).resolve()
    
    try:
        return str(resolved_file.relative_to(resolved_root))
    except ValueError:
        # Provide detailed error message similar to Docker path handling
        raise CodeSceneCliError(
            f"File '{file_path}' (resolved: {resolved_file}) "
            f"is not under git root '{git_root}' (resolved: {resolved_root}). "
            f"Ensure the file exists within the repository."
        )

def _is_windows_drive_letter(path: str) -> bool:
    """
    True if the path starts with a Windows drive letter, e.g. 'C:\\...'
    """
    return len(path) >= 2 and path[1] == ':'

def _extract_windows_drive_letter(p: str) -> str | None:
    if _is_windows_drive_letter(p):
        return p[0].upper()
    return None

def normalize_path(path: str) -> Path:
    """
    Normalize a file path to a POSIX-style Path object.
    Handles both Windows and POSIX paths.
    """
    normalized = PureWindowsPath(path).as_posix()
    windows_drive = _extract_windows_drive_letter(path)
    if windows_drive:
        normalized = f'/{windows_drive}/' + normalized[2:]
    return Path(normalized)

def _find_first_mismatch(a: tuple, b: tuple) -> int | None:
    """
    Returns the index of the first mismatched segment, or None if all segments match.
    Example: a = ('/', 'C', 'foo'), b = ('/', 'D', 'foo') -> returns 1
    """
    for index, (left, right) in enumerate(zip(a, b)):
        if left != right:
            return index

    has_extra_segments = len(a) != len(b)
    if has_extra_segments:
        # the first extra segment is the point of divergence:
        point_of_divergence = min(len(a), len(b))
        return point_of_divergence
    return None

def path_mismatch_error(file_path: Path, mount_path: Path) -> CodeSceneCliError:
    file_parts, mount_parts = file_path.parts, mount_path.parts
    idx = _find_first_mismatch(file_parts, mount_parts)
    if idx is not None:
        user_segment = file_parts[idx] if idx < len(file_parts) else '<none>'
        mount_segment = mount_parts[idx] if idx < len(mount_parts) else '<none>'
        suggestion = (
            f"Path mismatch at segment {idx}: "
            f"'{user_segment}' (input) vs '{mount_segment}' (mount). "
            f"Check for case sensitivity or typos. "
            f"To fix: ensure your CS_MOUNT_PATH matches the input path exactly."
        )
    else:
        suggestion = (
            "file_path is not under CS_MOUNT_PATH. "
            "Check for typos or incorrect mount configuration."
        )
    return CodeSceneCliError(
        f"file_path is not under CS_MOUNT_PATH: {str(file_path)!r}. {suggestion}"
    )

def _relative_path_under_mount(file_path: Path, mount_path: Path) -> Path:
    """
    Returns the path of file_path relative to mount_path, or raises a detailed error.
    """
    try:
        return file_path.relative_to(mount_path)
    except ValueError:
        raise path_mismatch_error(file_path, mount_path)

def adapt_mounted_file_path_inside_docker(file_path: str) -> str:
    """
    Convert a host-mounted absolute file path into the path the container sees.
    Requires CS_MOUNT_PATH env var. Returns POSIX path rooted at '/mount'.
    """
    mount = os.getenv("CS_MOUNT_PATH")
    if not mount:
        raise CodeSceneCliError("CS_MOUNT_PATH not defined.")

    mount_path = normalize_path(mount)
    user_path = normalize_path(file_path)

    if not user_path.is_absolute():
        raise CodeSceneCliError(f"file_path must be absolute: {file_path!r}")

    relative = _relative_path_under_mount(user_path, mount_path)
    if relative == Path("."):
        return "/mount"
    return f"/mount/{relative.as_posix()}"


def _read_worktree_gitdir(git_path: str) -> str | None:
    """
    If git_path points to a worktree .git file (not a directory),
    read and return the gitdir path it contains.
    
    Returns None if git_path is a directory (regular repo) or doesn't exist.
    """
    from pathlib import Path
    git_file = Path(git_path)
    
    if not git_file.exists() or git_file.is_dir():
        return None
    
    try:
        content = git_file.read_text(encoding="utf-8").strip()
        if content.startswith("gitdir:"):
            return content[7:].strip()
    except (IOError, OSError):
        pass
    
    return None


def get_worktree_gitdir(git_root_path: str) -> str | None:
    """
    Detect if git_root_path is a git worktree and return the gitdir path.
    
    Git worktrees have a .git file (not directory) containing a path like:
        gitdir: /path/to/main-repo/.git/worktrees/my-branch
    
    This function reads that .git file and returns the gitdir path if present.
    Works for both Docker and static modes - returns the raw path without
    any Docker path translation.
    
    Args:
        git_root_path: Path to the git repository root directory
        
    Returns:
        The gitdir path if this is a worktree, None if it's a regular repo
    """
    git_file = os.path.join(git_root_path, '.git')
    return _read_worktree_gitdir(git_file)


def get_relative_file_path_for_api(file_path: str) -> str:
    """
    Get a relative file path suitable for CodeScene API calls.
    
    This function converts paths to repository-relative format for API filtering.
    It handles three scenarios:
    
    1. Docker mode (CS_MOUNT_PATH set): Converts host path to container path
       and strips the '/mount/' prefix.
    2. Already relative path: Returns as-is.
    3. Absolute path without CS_MOUNT_PATH: Tries git root detection, falls back
       to returning the path unchanged if not in a git repository.
    
    Args:
        file_path: Path to the source code file (absolute or relative).
        
    Returns:
        A relative path string suitable for API filtering.
    """
    # Docker mode - use mount path logic
    if os.getenv("CS_MOUNT_PATH"):
        mounted_path = adapt_mounted_file_path_inside_docker(file_path)
        return mounted_path.lstrip("/mount/")
    
    path = Path(file_path)
    
    # Already relative - use as-is
    if not path.is_absolute():
        return file_path
    
    # Absolute path - try git root, but don't require it
    try:
        from .code_health_analysis import find_git_root
        git_root = find_git_root(file_path)
        return get_relative_path_from_git_root(file_path, git_root)
    except Exception:
        # Not in a git repo or git detection failed - return path as-is
        # The API will do pattern matching, so an absolute path may still work
        # or the user may need to provide a relative path
        return file_path


def adapt_worktree_gitdir_for_docker(worktree_path: str) -> str | None:
    """
    For a git worktree, translate the gitdir path to work inside Docker.
    
    Git worktrees have a .git file (not directory) containing a pointer like:
        gitdir: /Users/david/project/.git/worktrees/my-branch
    
    This absolute host path won't work inside Docker. This function:
    1. Reads the .git file in the worktree
    2. Translates the gitdir path using CS_MOUNT_PATH
    3. Returns the Docker-internal path
    
    Returns None if not a worktree or translation not possible.
    """
    git_file_path = f"{worktree_path.rstrip('/')}/.git"
    gitdir = _read_worktree_gitdir(git_file_path)
    
    if not gitdir:
        return None
    
    # Translate the gitdir path the same way we translate file paths
    try:
        return adapt_mounted_file_path_inside_docker(gitdir)
    except CodeSceneCliError:
        # gitdir path is outside the mounted area - can't help
        return None
