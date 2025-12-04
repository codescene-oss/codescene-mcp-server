
import os
from pathlib import Path, PureWindowsPath
from errors import CodeSceneCliError

def normalize_path(path: str) -> Path:
    """
    Normalize a file path to a POSIX-style Path object.
    Handles both Windows and POSIX paths.
    """
    def get_windows_drive_letter(p: str) -> str | None:
        if len(p) >= 2 and p[1] == ':':
            return p[0].upper()
        return None
    normalized = PureWindowsPath(path).as_posix()
    drive = get_windows_drive_letter(path)
    if drive:
        normalized = f'/{drive}/' + normalized[2:]
    return Path(normalized)

def find_first_mismatch(a: tuple, b: tuple) -> int | None:
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
        first_extra_segment_index = min(len(a), len(b))
        return first_extra_segment_index
    return None

def path_mismatch_error(file_path: Path, mount_path: Path) -> CodeSceneCliError:
    file_parts, mount_parts = file_path.parts, mount_path.parts
    idx = find_first_mismatch(file_parts, mount_parts)
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

def relative_path_under_mount(file_path: Path, mount_path: Path) -> Path:
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

    relative = relative_path_under_mount(user_path, mount_path)
    if relative == Path("."):
        return "/mount"
    return f"/mount/{relative.as_posix()}"
