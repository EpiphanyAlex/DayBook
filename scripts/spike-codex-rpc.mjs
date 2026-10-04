#!/usr/bin/env node
// Zero-model, spike-only JSONL transport test. This is not a Codex backend.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const self = fileURLToPath(import.meta.url);
const MAX_LINE = 256;
const MAX_EVENTS = 4;

if (process.argv[2] === '--fake') {
  const mode = process.argv[3];
  if (mode === 'oversize') {
    process.stdout.write(`${'x'.repeat(MAX_LINE + 1)}\n`);
  } else if (mode === 'flood') {
    for (let index = 0; index <= MAX_EVENTS; index++) {
      process.stdout.write(`${JSON.stringify({ method: 'progress', params: { index } })}\n`);
    }
  } else if (mode === 'eof') {
    process.exit(0);
  } else if (mode === 'hang') {
    process.stdin.resume();
  } else {
    let input = '';
    let requests = [];
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', (chunk) => {
      input += chunk;
      while (input.includes('\n')) {
        const index = input.indexOf('\n');
        const line = input.slice(0, index);
        input = input.slice(index + 1);
        const message = JSON.parse(line);
        if (message.id === 'reverse-1') {
          assert.equal(message.error.code, -32601);
          for (const request of [...requests].reverse()) {
            process.stdout.write(`${JSON.stringify({ id: request.id, result: { method: request.method } })}\n`);
          }
          process.exit(0);
        }
        requests.push(message);
        if (requests.length === 2) {
          process.stdout.write(`${JSON.stringify({ method: 'turn/started', params: { turnId: 't' } })}\n`);
          process.stdout.write(`${JSON.stringify({ id: 'reverse-1', method: 'unknown/request', params: {} })}\n`);
        }
      }
    });
  }
} else {
  function connect(mode) {
    const child = spawn(process.execPath, [self, '--fake', mode], { stdio: ['pipe', 'pipe', 'pipe'] });
    let nextId = 1;
    let bytes = Buffer.alloc(0);
    let closed = false;
    const pending = new Map();
    const events = [];
    function fail(error) {
      if (closed) return;
      closed = true;
      for (const entry of pending.values()) {
        clearTimeout(entry.timer);
        entry.reject(error);
      }
      pending.clear();
      child.kill();
    }
    child.stdout.on('data', (chunk) => {
      bytes = Buffer.concat([bytes, chunk]);
      if (bytes.length > MAX_LINE && !bytes.includes(10)) return fail(new Error('line limit'));
      while (bytes.includes(10) && !closed) {
        const end = bytes.indexOf(10);
        if (end > MAX_LINE) return fail(new Error('line limit'));
        const line = bytes.subarray(0, end).toString('utf8');
        bytes = bytes.subarray(end + 1);
        let message;
        try { message = JSON.parse(line); } catch { return fail(new Error('invalid JSON')); }
        if ('id' in message && !('method' in message)) {
          const entry = pending.get(message.id);
          if (!entry) return fail(new Error('unmatched response'));
          pending.delete(message.id);
          clearTimeout(entry.timer);
          if ('error' in message) entry.reject(new Error('RPC error'));
          else entry.resolve(message.result);
        } else {
          if (events.length >= MAX_EVENTS) return fail(new Error('queue limit'));
          events.push(message);
          if ('id' in message) child.stdin.write(`${JSON.stringify({ id: message.id, error: { code: -32601, message: 'unsupported request' } })}\n`);
        }
      }
    });
    child.stdout.on('end', () => fail(new Error('EOF before response')));
    child.on('error', fail);
    return {
      events,
      child,
      request(method, timeoutMs = 500) {
        const id = nextId++;
        return new Promise((resolve, reject) => {
          const timer = setTimeout(() => fail(new Error('request timeout')), timeoutMs);
          pending.set(id, { resolve, reject, timer });
          child.stdin.write(`${JSON.stringify({ id, method, params: {} })}\n`);
        });
      },
      close() { fail(new Error('cancelled')); },
    };
  }

  test('out-of-order IDs, preceding notification and reverse request', async () => {
    const rpc = connect('normal');
    try {
      const [first, second] = await Promise.all([rpc.request('first'), rpc.request('second')]);
      assert.equal(first.method, 'first');
      assert.equal(second.method, 'second');
      assert.deepEqual(rpc.events.map((event) => event.method), ['turn/started', 'unknown/request']);
    } finally { rpc.close(); }
  });

  test('oversize line fails closed', async () => {
    const rpc = connect('oversize');
    try { await assert.rejects(rpc.request('x'), /line limit/); } finally { rpc.close(); }
  });

  test('event queue overflow fails closed', async () => {
    const rpc = connect('flood');
    try { await assert.rejects(rpc.request('x'), /queue limit/); } finally { rpc.close(); }
  });

  test('EOF and deadline remain distinct failures', async () => {
    const eof = connect('eof');
    try { await assert.rejects(eof.request('x'), /EOF before response/); } finally { eof.close(); }
    const hang = connect('hang');
    try { await assert.rejects(hang.request('x', 50), /request timeout/); } finally { hang.close(); }
  });

  test('cancel rejects an in-flight request and reaps the fake CLI', async () => {
    const rpc = connect('hang');
    const waiting = rpc.request('x');
    rpc.close();
    await assert.rejects(waiting, /cancelled/);
    await new Promise((resolve) => rpc.child.once('close', resolve));
    assert.equal(rpc.child.exitCode === null && rpc.child.signalCode === null, false);
  });
}
