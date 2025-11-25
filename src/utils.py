import os
import requests

class CodeSceneCliError(Exception):
    """Raised when the CLI tool fails to calculate Code Health for a given file.
    """
    pass

def adapt_mounted_file_path_inside_docker(file_path):
    if not os.getenv("CS_MOUNT_PATH"):
        raise CodeSceneCliError("CS_MOUNT_PATH not defined.")

    mount_dir = os.getenv('CS_MOUNT_PATH').removesuffix('/')
    mount_file_path = file_path.replace(mount_dir, '/mount')

    return mount_file_path

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

    if data.get('max_pages') == 0:
        return items
    
    if data.get('max_pages') < params.get('page', 1):
        params['page'] = params.get('page', 1) + 1
        items.extend(query_api_list(endpoint, params, key))
        
    return items