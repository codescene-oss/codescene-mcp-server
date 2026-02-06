import os

import requests


def normalize_onprem_url(url: str) -> str:
    if url.endswith("/"):
        url = url[:-1]

    return url


def get_api_url() -> str:
    url = normalize_onprem_url(os.getenv("CS_ONPREM_URL") or "")
    return f"{url}/api" if url else "https://api.codescene.io"


def get_api_request_headers() -> dict:
    if os.getenv("CS_ACCESS_TOKEN") is None:
        return {}

    return {"Authorization": f"Bearer {os.getenv('CS_ACCESS_TOKEN')}"}


def query_api(endpoint, params: dict) -> dict:
    url = f"{get_api_url()}/{endpoint}"
    response = requests.get(url, params=params, headers=get_api_request_headers())

    return response.json()


def query_api_list(endpoint, params: dict, key: str) -> list:
    data = query_api(endpoint, params)
    items = data.get(key, [])

    if data.get("max_pages") == 0 or data.get("max_pages") is None:
        return items

    if data.get("max_pages") > data.get("page", 1):
        params["page"] = data.get("page", 1) + 1
        items.extend(query_api_list(endpoint, params, key))

    return items
