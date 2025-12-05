import os
import requests

class HttpError(Exception):
    pass

def get_api_url() -> str:
    return "https://devtools.codescene.io"

def get_api_request_headers() -> dict:
    if os.getenv("CS_PAT") is None:
        return {}

    return {
        'Authorization': f"Bearer {os.getenv('CS_PAT')}"
    }

def query_api(endpoint, params: dict) -> dict:
    url = f"{get_api_url()}/{endpoint}"
    response = requests.get(url, params=params, headers=get_api_request_headers())

    return response.json()

def post(endpoint, json_payload: dict) -> dict:
    url = f"{get_api_url()}/{endpoint}"
    return requests.post(url, json=json_payload, headers=get_api_request_headers())

def retrying_post(n, endpoint, json_payload: dict) -> dict:
  r = post(endpoint, json_payload)
  if r.ok:
    return r.json()
  if 1 < n and r.status in [408, 504]:
    print("Retry post...")
    return retrying_post(n - 1, endpoint, json_payload)
  raise HttpError("HttpError")

def post_refactor(json_payload: dict) -> dict:
    return retrying_post(3, 'api/refactor', json_payload)