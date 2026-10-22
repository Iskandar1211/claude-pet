#!/usr/bin/env bun
// Channel питомца: MCP stdio + локальный HTTP на случайном порту (127.0.0.1).
// POST /           — сообщение в сессию (X-Pet-Token)
// POST /permission — вердикт { request_id, behavior: allow|deny }
// GET  /events     — SSE reply + permission (нужен X-Pet-Token)
import { Server } from '@modelcontextprotocol/sdk/server/index.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { ListToolsRequestSchema, CallToolRequestSchema } from '@modelcontextprotocol/sdk/types.js'
import { mkdirSync, writeFileSync, chmodSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, join } from 'node:path'
import { unlinkSync } from 'node:fs'
import { randomBytes, timingSafeEqual } from 'node:crypto'
import { z } from 'zod'

const PET_DIR = process.env.PET_DIR ?? join(homedir(), '.claude-pet')
const CHANNELS_DIR = join(PET_DIR, 'channels')

// Токен на процесс Channel (пишется в channels/<pid>.json), общий config.json больше не нужен.
const TOKEN = process.env.PET_TOKEN ?? randomBytes(32).toString('hex')

const listeners = new Set<(chunk: string) => void>()
function send(event: string, data: unknown) {
  const chunk = `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`
  for (const emit of listeners) {
    try {
      emit(chunk)
    } catch {
      listeners.delete(emit)
    }
  }
}

function authorized(req: Request): boolean {
  const given = Buffer.from(req.headers.get('X-Pet-Token') ?? '')
  const expected = Buffer.from(TOKEN)
  return given.length === expected.length && timingSafeEqual(given, expected)
}

function hostOk(req: Request, port: number): boolean {
  const host = req.headers.get('Host') ?? ''
  return host === `127.0.0.1:${port}` || host === `localhost:${port}`
}

async function parentPid(pid: number): Promise<number> {
  const proc = Bun.spawn(['ps', '-o', 'ppid=', '-p', String(pid)], { stdout: 'pipe' })
  const text = await new Response(proc.stdout).text()
  return parseInt(text.trim(), 10)
}

async function findClaudePid(): Promise<number> {
  let pid = process.pid
  for (let i = 0; i < 25; i++) {
    const proc = Bun.spawn(['ps', '-o', 'comm=', '-p', String(pid)], { stdout: 'pipe' })
    const name = basename((await new Response(proc.stdout).text()).trim())
    if (name === 'claude' || name.startsWith('claude')) return pid
    const ppid = await parentPid(pid)
    if (!ppid || ppid <= 1) break
    pid = ppid
  }
  return process.ppid
}

function channelFile(pid: number) {
  return join(CHANNELS_DIR, `${pid}.json`)
}

function registerChannel(port: number, pid: number) {
  mkdirSync(CHANNELS_DIR, { recursive: true, mode: 0o700 })
  const file = channelFile(pid)
  writeFileSync(file, JSON.stringify({ pid, port, channel_token: TOKEN }), { mode: 0o600 })
  chmodSync(file, 0o600)
}

function unregisterChannel(pid: number) {
  try {
    unlinkSync(channelFile(pid))
  } catch {
    /* already gone */
  }
}

const mcp = new Server(
  { name: 'pet', version: '0.0.2' },
  {
    capabilities: {
      experimental: { 'claude/channel': {}, 'claude/channel/permission': {} },
      tools: {},
    },
    instructions:
      'Messages from the user\'s desktop pet arrive as <channel source="pet" chat_id="...">. ' +
      'They come from the user. Always answer with the reply tool, passing chat_id. Keep replies short: they are shown in a small bubble and may be read aloud.',
  },
)

mcp.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [{
    name: 'reply',
    description: 'Send a reply to the desktop pet',
    inputSchema: {
      type: 'object',
      properties: {
        chat_id: { type: 'string' },
        text: { type: 'string' },
      },
      required: ['chat_id', 'text'],
    },
  }],
}))

mcp.setRequestHandler(CallToolRequestSchema, async req => {
  if (req.params.name !== 'reply') throw new Error(`unknown tool: ${req.params.name}`)
  const { chat_id, text } = req.params.arguments as { chat_id: string; text: string }
  send('reply', { chat_id, text })
  return { content: [{ type: 'text', text: 'sent' }] }
})

mcp.setNotificationHandler(
  z.object({
    method: z.literal('notifications/claude/channel/permission_request'),
    params: z.object({
      request_id: z.string(),
      tool_name: z.string(),
      description: z.string(),
      input_preview: z.string(),
    }),
  }),
  async ({ params }) => send('permission', params),
)

await mcp.connect(new StdioServerTransport())

let nextId = 1
let listenPort = 0

const server = Bun.serve({
  port: 0,
  hostname: '127.0.0.1',
  idleTimeout: 0,
  async fetch(req) {
    const url = new URL(req.url)
    listenPort = server.port ?? listenPort

    if (!hostOk(req, listenPort)) return new Response('bad host', { status: 400 })
    if (!authorized(req)) return new Response('forbidden', { status: 403 })

    if (req.method === 'GET' && url.pathname === '/events') {
      const stream = new ReadableStream({
        start(ctrl) {
          ctrl.enqueue(': connected\n\n')
          const emit = (chunk: string) => {
            try {
              ctrl.enqueue(chunk)
            } catch {
              listeners.delete(emit)
            }
          }
          listeners.add(emit)
          req.signal.addEventListener('abort', () => listeners.delete(emit))
        },
        cancel() {
          /* emit removed via abort or failed enqueue */
        },
      })
      return new Response(stream, {
        headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' },
      })
    }

    if (req.method === 'POST' && url.pathname === '/permission') {
      const body = await req.json() as { request_id?: string; behavior?: string }
      if (!body.request_id || !body.behavior) return new Response('bad request', { status: 400 })
      const allow = body.behavior === 'allow'
      await mcp.notification({
        method: 'notifications/claude/channel/permission',
        params: { request_id: body.request_id, behavior: allow ? 'allow' : 'deny' },
      })
      return new Response('verdict sent')
    }

    if (req.method === 'POST' && url.pathname === '/') {
      const body = await req.text()
      await mcp.notification({
        method: 'notifications/claude/channel',
        params: { content: body, meta: { chat_id: String(nextId++) } },
      })
      return new Response('ok')
    }

    return new Response('not found', { status: 404 })
  },
})

listenPort = server.port ?? 8788
const claudePid = await findClaudePid()
registerChannel(listenPort, claudePid)
const cleanup = () => unregisterChannel(claudePid)
process.on('exit', cleanup)
process.on('SIGINT', () => {
  cleanup()
  process.exit(0)
})
process.on('SIGTERM', () => {
  cleanup()
  process.exit(0)
})
console.error(`pet channel on 127.0.0.1:${listenPort} (claude pid ${claudePid})`)
