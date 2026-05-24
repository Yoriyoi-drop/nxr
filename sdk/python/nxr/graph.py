from .client import NxrClient
from typing import Optional


class GraphStore:
    def __init__(self, client: NxrClient):
        self._client = client

    def add_node(self, label: str, properties: dict = None) -> int:
        if properties is None:
            properties = {}
        props_str = ",".join(f"{k}={v}" for k, v in properties.items())
        resp = self._client.send(f"GADD {label} {props_str}")
        if resp.startswith("OK:"):
            return int(resp[3:])
        raise RuntimeError(f"Add node failed: {resp}")

    def add_edge(self, from_node: int, to_node: int, relation: str, weight: float = 1.0) -> int:
        raise NotImplementedError("Edge not available via simple protocol")

    def get_node(self, id: int) -> Optional[dict]:
        raise NotImplementedError("Node detail not available via simple protocol")

    def find_by_label(self, label: str) -> list[dict]:
        raise NotImplementedError("Label search not available via simple protocol")
