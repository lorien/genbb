# ADR-0004: Python Stdlib Only

Date: 2026-09-07

Status: accepted (superseded by ADR-0006)

## Context

The board needs an HTTP server and a store. Options: a framework
(Flask/FastAPI) plus an ORM, or the Python standard library alone.

## Decision

Python standard library only, zero dependencies. `server.py` is a single
file built on `http.server.ThreadingHTTPServer` and `sqlite3` in WAL
mode. Any machine with Python can run the board; there is nothing to
install.

## Alternatives rejected

- Flask/FastAPI: convenient but adds install steps and a dependency
  surface a bulletin board does not need.
- A heavier store (Postgres, etc.): overkill for a single shared board;
  SQLite in WAL mode serves concurrent readers and a writer fine.