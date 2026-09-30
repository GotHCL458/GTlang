import sys
import pytest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from helpers import COMPILER, compile_vix, compile_and_run, compile_source


def pytest_configure(config):
    config.addinivalue_line("markers", "unit: Unit tests for compiler internals")
    config.addinivalue_line("markers", "integration: Integration tests (compile + run)")
    config.addinivalue_line("markers", "fuzz: Fuzz testing")
    config.addinivalue_line("markers", "stress: Stress and load tests")
    config.addinivalue_line("markers", "cli: Command-line interface tests")
    config.addinivalue_line("markers", "error: Error handling and diagnostics tests")
    config.addinivalue_line("markers", "feature: Feature-specific tests")
    config.addinivalue_line("markers", "slow: Tests that take a long time to run")


@pytest.fixture(scope="session")
def compiler():
    if not COMPILER.exists():
        pytest.skip(f"Compiler not found at {COMPILER}")
    return COMPILER


@pytest.fixture(scope="session")
def root_dir():
    return Path(__file__).resolve().parent.parent


@pytest.fixture
def compile_helper(compiler, tmp_path):
    def _helper(source_code: str, extra_args=None):
        return compile_and_run(compiler, source_code, tmp_path, extra_args)
    return _helper


@pytest.fixture
def compile_only(compiler, tmp_path):
    def _helper(source_code: str, extra_args=None):
        return compile_source(compiler, source_code, tmp_path, extra_args)
    return _helper
