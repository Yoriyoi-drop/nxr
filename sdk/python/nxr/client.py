import socket
import json
from typing import Optional, Any


class NxrClient:
    def __init__(self, host: str = "127.0.0.1", port: int = 9643):
        self.addr = (host, port)
        self.vector = VectorStore(self)
        self.graph = GraphStore(self)
        self.kv = KvStore(self)
        self._conn: Optional[socket.socket] = None

    def connect(self) -> None:
        self._conn = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._conn.connect(self.addr)
        self._conn.settimeout(30)

    def close(self) -> None:
        if self._conn:
            self._conn.close()
            self._conn = None

    def send(self, command: str) -> str:
        if not self._conn:
            self.connect()
        assert self._conn is not None
        self._conn.sendall(f"{command}\n".encode("utf-8"))
        data = self._conn.recv(65536)
        return data.decode("utf-8").strip()

    def ping(self) -> bool:
        resp = self.send("PING")
        return resp == "PONG"

    def query(self, ql: str) -> Any:
        resp = self.send(f"QUERY {ql}")
        try:
            return json.loads(resp)
        except json.JSONDecodeError:
            return resp

    def stats(self) -> dict:
        resp = self.send("STATS")
        try:
            return json.loads(resp)
        except json.JSONDecodeError:
            return {"raw": resp}

    def __enter__(self):
        self.connect()
        return self

    def __exit__(self, *args):
        self.close()


class VectorStore:
    def __init__(self, client: NxrClient):
        self._client = client

    def insert(self, id: int, vector: list[float], metadata: bytes = b"") -> bool:
        dim = len(vector)
        vals = " ".join(str(v) for v in vector)
        resp = self._client.send(f"VINSERT {id} {dim} {vals}")
        return resp == "OK"

    def search(self, query: list[float], k: int = 10) -> list[tuple[int, float]]:
        vals = ",".join(str(v) for v in query)
        resp = self._client.send(f"VSEARCH {vals}")
        results = []
        if resp and not resp.startswith("ERROR"):
            for part in resp.split(","):
                if ":" in part:
                    id_str, score_str = part.split(":")
                    results.append((int(id_str), float(score_str)))
        return results


class GraphStore:
    def __init__(self, client: NxrClient):
        self._client = client

    def add_node(self, label: str, properties: dict[str, str] = None) -> int:
        if properties is None:
            properties = {}
        props_str = ",".join(f"{k}={v}" for k, v in properties.items())
        resp = self._client.send(f"GADD {label} {props_str}")
        if resp.startswith("OK:"):
            return int(resp[3:])
        raise RuntimeError(f"Failed to add node: {resp}")

    def add_edge(self, from_node: int, to_node: int, relation: str, weight: float = 1.0) -> int:
        raise NotImplementedError("Use client.send directly")

    def get_node(self, id: int) -> dict:
        raise NotImplementedError("Use client.send directly")


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
