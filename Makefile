.PHONY: all build run stop clean restart cli python-shell logs shell

# ─── Docker ───────────────────────────────────────────────
build:
	docker build -t nxr-db:latest .

run: build
	docker-compose up -d nxr-db

stop:
	docker-compose down

restart: stop run

clean: stop
	docker-compose down -v
	docker rmi nxr-db:latest 2>/dev/null || true

logs:
	docker-compose logs -f nxr-db

shell:
	docker-compose run --rm nxr-cli sh

# ─── CLI via Docker ──────────────────────────────────────
cli:
	docker-compose run --rm nxr-cli $(filter-out $@,$(MAKECMDGOALS))

# ─── Python SDK ──────────────────────────────────────────
python-shell:
	docker build -t nxr-db:latest .
	docker run --rm -it --network container:nxr-db \
		-v $$(pwd)/sdk/python:/sdk \
		python:3.12-alpine sh -c " \
			pip install /opt/nxr-sdk.whl && \
			cd /sdk && \
			python examples/basic_usage.py"

# ─── Local (non-Docker) ──────────────────────────────────
local-build:
	cargo build --release

local-run: local-build
	RUST_LOG=info ./target/release/nxrd --db-path /tmp/nxr-db --bind 127.0.0.1:9643

local-init:
	mkdir -p /tmp/nxr-db/{vectors/segments,graph,kv/cold,wal,indexes,snapshots,logs}
	cp nxr-db/config.toml /tmp/nxr-db/config.toml

local-test:
	@echo "Running integration tests..."
	python3 sdk/python/examples/basic_usage.py

# ─── Info ────────────────────────────────────────────────
help:
	@echo "NXR Database — Makefile"
	@echo ""
	@echo "Docker:"
	@echo "  make build       Build Docker image"
	@echo "  make run         Start container"
	@echo "  make stop        Stop container"
	@echo "  make logs        Follow logs"
	@echo "  make shell       Shell inside container"
	@echo "  make cli CMD     Run nxr CLI (e.g. 'make cli stats')"
	@echo "  make clean       Remove container + image"
	@echo ""
	@echo "Local:"
	@echo "  make local-build    Build Rust binary"
	@echo "  make local-init     Init local DB"
	@echo "  make local-run      Run daemon locally"
	@echo "  make local-test     Run Python SDK test"
