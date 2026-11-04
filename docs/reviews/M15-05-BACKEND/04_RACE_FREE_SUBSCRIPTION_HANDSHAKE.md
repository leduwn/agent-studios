# 04 — Race-Free Subscription Handshake

## 1. The Concurrency Challenge

When a client executes `SubscribeEvents { from_sequence: K + 1 }`, the server must:
1. Replay historical events `[K + 1 .. M]` that were previously committed.
2. Stream newly emitted live events `[M + 1, M + 2, ...]`.

If the server replays historical events first and then subscribes to the live broadcast channel, any event emitted between the end of historical replay and the start of live subscription is permanently lost (a **boundary race gap**). Conversely, if the server subscribes first without sequence filtering, the client receives duplicate events.

---

## 2. The Four-Phase Race-Free Handshake Protocol

To guarantee zero event loss and zero event duplication, `crates/ipc-transport/src/connection.rs` implements a four-phase subscription protocol:

```
[ Phase 1: Tap Live Feed ]
   • Acquire broadcast receiver: let mut broadcast_rx = server_state.broadcast_tx.subscribe();
   • Read current buffer watermark: let max_buffered_seq = buf.last_sequence().unwrap_or(0);

[ Phase 2: Historical Backfill ]
   • Query ReplayBuffer: buf.replay_from(req_seq, None)
   • On SequenceGapUnrecoverable:
       - Fall back to SQLite: cp.events_slice(offset, count)
       - Stream historical slice to client via tx.send()
       - Record last_delivered_seq

[ Phase 3: In-Memory Tail Drain ]
   • If last_delivered_seq < max_buffered_seq:
       - Drain remaining intermediate events [last_delivered_seq + 1 ..= max_buffered_seq] from ReplayBuffer
       - Advance last_delivered_seq = max_buffered_seq

[ Phase 4: Filtered Live Stream Transition ]
   • Spawn streaming loop with watermark:
       let mut current_seq = if last_delivered_seq > 0 {
           last_delivered_seq
       } else {
           (req_seq - 1).min(max_buffered_seq)
       };
   • In loop:
       tokio::select! {
           res = broadcast_rx.recv() => {
               if event.sequence <= current_seq {
                   continue; // Silently drop historical frames caught in Phase 1 tap
               }
               // Gap check against dropped frames or lagged receiver
               if event.sequence > current_seq + 1 {
                   backfill_gap(current_seq + 1, event.sequence - 1);
               }
               current_seq = event.sequence;
               tx.send(IpcResponse::Event { event }).await;
           }
       }
```

---

## 3. Mathematical Proof of Monotonicity & Zero-Loss

Let:
- $K$: Client's requested start sequence (`from_sequence = K`).
- $T_0$: Instant when `broadcast_rx = broadcast_tx.subscribe()` is called.
- $W_0$: Maximum sequence committed at $T_0$ (`max_buffered_seq`).
- $E_{hist}$: Events delivered during backfill phases 2 and 3.

### Invariant 1: No Event Loss
- Any event emitted prior to $T_0$ has sequence $\le W_0$ and is committed to SQLite or `ReplayBuffer`. Phase 2 and Phase 3 query SQLite and `ReplayBuffer` up to $W_0$.
- Any event emitted after $T_0$ is captured by `broadcast_rx`.
- Since $T_0$ precedes the snapshot of $W_0$, the sets of events cover the entire sequence range $[K, \infty)$ with no temporal gap.

### Invariant 2: No Duplication
- During Phase 4, the initial watermark is set to $current\_seq = last\_delivered\_seq$ (where $last\_delivered\_seq \ge W_0$).
- When `broadcast_rx.recv()` yields an event with sequence $S$:
  - If $S \le current\_seq$, it was already delivered during Phase 2 or 3; the condition `if event.sequence <= current_seq { continue; }` drops it.
  - If $S = current\_seq + 1$, it is delivered and $current\_seq$ advances to $S$.
- Therefore, every sequence $S \ge K$ is delivered to the client exactly once.

---

## 4. Receiver Lag Recovery (`RecvError::Lagged`)

If a slow client or saturated socket causes the internal broadcast channel buffer to overflow, Tokio returns `RecvError::Lagged(skipped_count)`:
```rust
Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
    let gap_start = current_seq + 1;
    let gap_events = {
        let buf = replay_buffer_arc.lock().unwrap();
        buf.replay_from(gap_start, Some(skipped as usize))
    };
    // If ReplayBuffer has already evicted the gap, fall back to SQLite cp.events_slice
    ...
}
```
The connection task intercepts the lagged error, queries `ReplayBuffer` or `ControlPlane::events_slice`, catches up the missing events in sequential order, and resumes the live broadcast feed seamlessly.
