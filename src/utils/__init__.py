import os
from pathlib import Path
import requests
from errors import CodeSceneCliError


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

    p = Path(file_path)
    if not p.is_absolute():
        raise CodeSceneCliError(f"file_path must be absolute: {file_path!r}")

    def relative_path_under_mount(file_path: Path, mount_path: Path) -> Path:
        try:
            return file_path.relative_to(mount_path)
        except ValueError:
            raise CodeSceneCliError(f"file_path is not under CS_MOUNT_PATH: {str(file_path)!r}")

    relative = relative_path_under_mount(p, Path(mount))

    # If the file points to the mount root, relative_to yields '.'
    if relative == Path("."):
        return "/mount"

    mount_posix_style = "/mount/" + relative.as_posix()
    return mount_posix_style

def get_api_url() -> str:
    url = os.getenv("CS_ONPREM_URL")
    return f"{url}/api" if url else "https://api.codescene.io"

def get_api_request_headers() -> dict:
    if os.getenv("CS_ACCESS_TOKEN") is None:
        return {}

    return {
        'Authorization': f"Bearer {os.getenv('CS_ACCESS_TOKEN')}"
    }

def query_api_list(endpoint, params: dict, key: str) -> list:
    url = f"{get_api_url()}/{endpoint}"
    response = requests.get(url, params=params, headers=get_api_request_headers())
    data = response.json()
    items = data.get(key, [])

    if data.get('max_pages') == 0 or data.get('max_pages') is None:
        return items

    if data.get('max_pages') > data.get('page', 1):
        params['page'] = data.get('page', 1) + 1
        items.extend(query_api_list(endpoint, params, key))

    return items