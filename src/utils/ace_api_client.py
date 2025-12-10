import os
import requests

class BadRequest(Exception):
    pass

def get_api_url() -> str:
    return "https://devtools.codescene.io"

def get_api_request_headers() -> dict:
    if os.getenv("CS_ACE_ACCESS_TOKEN") is None:
        return {}

    return {
        'Authorization': f"Bearer {os.getenv('CS_ACE_ACCESS_TOKEN')}"
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
  if 1 < n and r.status_code in [408, 504]:
    print("Retry post...")
    return retrying_post(n - 1, endpoint, json_payload)
  
  if r.status_code == 400:
    # ACE provides error details as json on bad requests
    raise BadRequest(f"Bad request: {r.json()}")
  
  r.raise_for_status()

def post_refactor(json_payload: dict) -> dict:
    return retrying_post(3, 'api/refactor', json_payload)