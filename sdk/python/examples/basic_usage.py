from nxr import NxrClient

def main():
    # Connect to NXR database
    client = NxrClient("127.0.0.1", 9643)

    with client:
        # Ping
        print(f"Server alive: {client.ping()}")

        # Insert vector (1536 dimensions for demo, using 4)
        vector = [0.1, 0.2, 0.3, 0.4]
        ok = client.vector.insert(1, vector)
        print(f"Vector inserted: {ok}")

        # Search similar vectors
        results = client.vector.search([0.1, 0.15, 0.3, 0.35])
        print(f"Search results: {results}")

        # Add graph node
        node_id = client.graph.add_node("User", {"name": "Alice", "age": "30"})
        print(f"Graph node created: {node_id}")

        # KV operations
        client.kv.set("session:alice", '{"token": "abc123"}', ttl=3600)
        val = client.kv.get("session:alice")
        print(f"KV get: {val}")

        # Stats
        stats = client.stats()
        print(f"Database stats: {stats}")


if __name__ == "__main__":
    main()
