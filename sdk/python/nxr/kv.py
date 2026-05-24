from .client import NxrClient
from typing import Optional


class KvStore:
    def __init__(self, client: NxrClient):
        self._client = client

    def get(self, key: str) -> Optional[bytes]:
        resp = self._client.send(f"KVGET {key}")
        if resp == "NOT_FOUND":
            return None
        if resp.startswith("ERROR"):
            raise RuntimeError(resp)
        return resp.encode("utf-8")

    def set(self, key: str, value: str, ttl: int = 0) -> bool:
        resp = self._client.send(f"KVSET {key} {value} {ttl}")
        return resp == "OK"

    def delete(self, key: str) -> bool:
        raise NotImplementedError("KV delete not available via simple protocol")
