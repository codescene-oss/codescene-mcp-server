import os
from pathlib import Path, PureWindowsPath
from errors import CodeSceneCliError

def normalize_path(path: str) -> str:
    """
    Normalize a file path to a POSIX-style Path object.
    Converts Windows-style paths to POSIX-style.
    
    Note that while it always uses PureWindowsPath for normalization,
    it does not assume the input is necessarily a Windows path and 
    works fine with POSIX paths as well.
    """
    def get_windows_drive_letter(path: str) -> str | None:
        if len(path) >= 2 and path[1] == ':':    
            return path[0].upper()
        return None
    
    normalized_path = PureWindowsPath(path).as_posix()

    if drive := get_windows_drive_letter(path):
        normalized_path = f'/{drive}/' + normalized_path[2:]

    return Path(normalized_path)

def adapt_mounted_file_path_inside_docker(file_path: str) -> str:
    """
    Convert a host-mounted absolute file path into the path the container sees.

    - Requires the environment variable `CS_MOUNT_PATH` to be set.
    - `file_path` must be absolute and located under `CS_MOUNT_PATH`.
    - Returns a POSIX-style path rooted at '/mount' (e.g. '/mount/src/foo.py').
    """
    mount = os.getenv("CS_MOUNT_PATH")
    if not mount:
        raise CodeSceneCliError("CS_MOUNT_PATH not defined.")
    
    normalized_mount_path = normalize_path(mount)
    normalized_file_path = normalize_path(file_path)

    if not normalized_file_path.is_absolute():
        raise CodeSceneCliError(f"file_path must be absolute: {file_path!r}")

    def relative_path_under_mount(file_path: Path, mount_path: Path) -> Path:
        try:
            return file_path.relative_to(mount_path)
        except ValueError:
            raise CodeSceneCliError(f"file_path is not under CS_MOUNT_PATH: {str(file_path)!r}")

    relative = relative_path_under_mount(normalized_file_path, normalized_mount_path)

    # If the file points to the mount root, relative_to yields '.'
    if relative == Path("."):
        return "/mount"

    mount_posix_style = "/mount/" + relative.as_posix()
    return mount_posix_style
