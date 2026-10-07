import sqlite3
from pathlib import Path

import pytest

FIXTURES = Path(__file__).parent / "fixtures"


@pytest.fixture
def sysfs():
    return lambda name: FIXTURES / "sysfs" / name


@pytest.fixture
def procfs():
    return FIXTURES / "proc"


@pytest.fixture(autouse=True)
def close_sqlite_connections(monkeypatch):
    """Close every connection a test opened, so none leak into other tests."""
    opened = []
    real_connect = sqlite3.connect

    def tracking_connect(*args, **kwargs):
        conn = real_connect(*args, **kwargs)
        opened.append(conn)
        return conn

    monkeypatch.setattr(sqlite3, "connect", tracking_connect)
    yield
    for conn in opened:
        conn.close()
