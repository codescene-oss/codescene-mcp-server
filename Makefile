create-executable:
	python3.13 -m nuitka --onefile \
	--assume-yes-for-downloads \
	--include-data-dir=./src/docs=src/docs \
	--include-data-files=./cs=cs \
	--output-filename=cs-mcp \
	src/cs_mcp_server.py

.PHONY: test-integration
test-integration:
	@echo "Running comprehensive integration tests..."
	./tests/run-integration-tests.sh

.PHONY: test-integration-platform
test-integration-platform:
	@echo "Running platform-specific integration tests..."
	@if [ ! -f "../cs_mcp_test_bin/cs-mcp" ]; then \
		echo "No executable found. Building first..."; \
		./tests/run-integration-tests.sh; \
	else \
		./tests/run-integration-tests.sh --platform-only --executable ../cs_mcp_test_bin/cs-mcp; \
	fi

.PHONY: test-integration-worktree
test-integration-worktree:
	@echo "Running git worktree integration tests..."
	@if [ ! -f "../cs_mcp_test_bin/cs-mcp" ]; then \
		echo "No executable found. Building first..."; \
		./tests/run-integration-tests.sh; \
	else \
		./tests/run-integration-tests.sh --worktree-only --executable ../cs_mcp_test_bin/cs-mcp; \
	fi

.PHONY: test-all
test-all:
	@echo "Running unit tests..."
	python3 -m pytest src/
	@echo ""
	@echo "Running integration tests..."
	./tests/run-integration-tests.sh

.PHONY: lint
lint:
	ruff check .

.PHONY: lint-fix
lint-fix:
	ruff check . --fix

.PHONY: format
format:
	ruff format .

.PHONY: format-check
format-check:
	ruff format . --check
