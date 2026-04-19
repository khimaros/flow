build: build-ui build-api
.PHONY: build

precommit: update-gitignore lint test
.PHONY: precommit

test: test-api-e2e test-ui-e2e test-cli-e2e
.PHONY: test

test-api-e2e:
	./tests/run_e2e.sh
.PHONY: test-api-e2e

test-cli-e2e:
	./tests/run_cli_tests.sh
.PHONY: test-cli-e2e

test-ui-e2e: build-ui
	./tests/run_ui_e2e.sh
.PHONY: test-ui-e2e

build-api:
	cargo build --workspace
.PHONY: build-api

build-ui:
	cd ui && npm run build
.PHONY: build-ui

deps:
	cd ui && npm install
	cargo install
.PHONY: deps

start: build-ui
	cargo run --bin flow-server
.PHONY: start

lint:
	cd ui && npm run lint
	cargo clippy
.PHONY: lint

update-gitignore:
	./scripts/update-gitignore-assets.sh
.PHONY: update-gitignore

bundle:
	./scripts/bundle-release.sh $(BUNDLE_ARGS)
.PHONY: bundle

format:
	cd ui && npm run format
	cargo fmt
	black user_nodes/
.PHONY: format
