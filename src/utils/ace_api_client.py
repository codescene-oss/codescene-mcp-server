import os
import requests

class HttpClientError(Exception):
    pass

def get_api_endpoint(endpoint: str) -> str:
    return f"https://devtools.codescene.io/{endpoint}"

def get_api_request_headers() -> dict:
    return {
        'Authorization': f"Bearer {os.getenv('CS_ACE_ACCESS_TOKEN')}"
    }

def post(endpoint: str, json_payload: dict) -> dict:
    return requests.post(get_api_endpoint(endpoint), json=json_payload, headers=get_api_request_headers())

def _is_client_error(response: requests.Response) -> bool:
    return response.status_code >= 400 and response.status_code < 500 

def _is_json(response: requests.Response) -> bool:
    return response.headers.get('content-type').find('application/json') >= 0

def retrying_post(n: int, endpoint: str, json_payload: dict) -> requests.Response:
    r = post(endpoint, json_payload)
    if 1 < n and r.status_code in [408, 504]:
        print("Retry post...")
        return retrying_post(n - 1, endpoint, json_payload)
    return r
  
def validate_response(response: requests.Response):
    if response.ok:
        return
  
    # devtools-portal and ACE provides error details as json on bad requests
    if _is_client_error(response) and _is_json(response):
        error = response.json().get('error', "<unknown-error>")
        raise HttpClientError(f"HttpClientError {response.status_code}: {error}")
  
    response.raise_for_status()

def post_refactor(json_payload: dict) -> dict:
    r = retrying_post(3, 'api/refactor', json_payload)
    validate_response(r)
    return r.json()

