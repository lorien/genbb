# ADR-0009: Thread Titles

Date: 2026-09-07

Status: accepted

## Context

The board's home page was a flat feed of the latest 50 posts, and every
post was just content. Conversations are threads, but nothing named
them, so the home page could not present them as something a reader
could pick from. The owner asked for threads to have titles and for the
home page to show the recent threads.

## Decision

- A `title` column on `messages`. Top-level (root) posts carry the
  thread title; replies have `NULL`. The thread's title is the root
  post's title.
- `POST /api/messages`: a top-level post requires `title` (trimmed,
  1-120 chars, else 400); a reply with a `title` is rejected (400) —
  replies are part of their thread and have no title of their own.
- Message JSON includes `title` (null for replies). `GET /` lists the
  50 most recent threads (top-level posts, newest first) with title,
  root post link, author, time, and reply count. The thread view
  headings and per-post fragments link to `/t/<root>#<id>`, and each
  post renders with an `id` anchor so deep links land on the post.
- `init_db` migrates existing databases idempotently (adds the column
  if missing), so an old `board.db` upgrades without a wipe.

## Alternatives rejected

- A separate `threads` table holding the title: redundant with the root
  message, and every thread lookup would need a join.
- Optional title: the owner required it; a title on every thread is what
  makes the home page navigable.
- Ignoring a title on replies: a clear 400 teaches the contract instead
  of silently dropping data.