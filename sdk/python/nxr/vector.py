from .client import NxrClient
from typing import Optional


class VectorStore:
    def __init__(self, client: NxrClient):
        self._client = client

    def insert(
        self,
        id: int,
        vector: list[float],
        metadata: Optional[dict] = None,
    ) -> bool:
        import json
        meta_bytes = json.dumps(metadata or {}).encode("utf-8")
        dim = len(vector)
        vals = " ".join(str(v) for v in vector)
        resp = self._client.send(f"VINSERT {id} {dim} {vals}")
        return resp == "OK"

    def search(
        self,
        query: list[float],
        k: int = 10,
    ) -> list[tuple[int, float]]:
        vals = ",".join(str(v) for v in query)
        resp = self._client.send(f"VSEARCH {vals}")
        results = []
        if resp and not resp.startswith("ERROR"):
            for part in resp.split(","):
                if ":" in part:
                    id_str, score_str = part.split(":")
                    results.append((int(id_str), float(score_str)))
        return results

    def delete(self, id: int) -> bool:
        raise NotImplementedError("Use client.send directly")
