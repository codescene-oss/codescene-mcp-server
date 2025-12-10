import os
import requests

class HttpClientError(Exception):
    pass

def get_api_url() -> str:
    return "https://devtools.codescene.io"

def get_api_request_headers() -> dict:
    if os.getenv("CS_ACE_ACCESS_TOKEN") is None:
        return {}

    return {
        'Authorization': f"Bearer {os.getenv('CS_ACE_ACCESS_TOKEN')}"
    }

def query_api(endpoint: str, params: dict) -> dict:
    url = f"{get_api_url()}/{endpoint}"
    response = requests.get(url, params=params, headers=get_api_request_headers())

    return response.json()

def post(endpoint: str, json_payload: dict) -> dict:
    url = f"{get_api_url()}/{endpoint}"
    return requests.post(url, json=json_payload, headers=get_api_request_headers())

def _is_client_error(response: requests.Response) -> bool:
   return response.status_code >= 400 and response.status_code < 500 

def _is_json(response: requests.Response) -> bool:
   return response.headers.get('content-type').find('application/json') >= 0

def retrying_post(n: int, endpoint: str, json_payload: dict) -> dict:
  r = post(endpoint, json_payload)
  if r.ok:
    return r.json()
  if 1 < n and r.status_code in [408, 504]:
    print("Retry post...")
    return retrying_post(n - 1, endpoint, json_payload)
  
  # devtools-portal and ACE provides error details as json on bad requests
  if _is_client_error(r) and _is_json(r):
    error = r.json()['error']
    raise HttpClientError(f"HttpClientError {r.status_code}: {error}")
  
  r.raise_for_status()

def post_refactor(json_payload: dict) -> dict:
    return retrying_post(3, 'api/refactor', json_payload)