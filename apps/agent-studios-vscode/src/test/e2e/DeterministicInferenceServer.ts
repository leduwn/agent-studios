/**
 * Deterministic Multi-Protocol Local Inference Server for MASTER MISSION 05 Acceptance Tests.
 *
 * Implements strict header-based auth verification (`Authorization: Bearer ...`, `x-api-key`,
 * `x-goog-api-key`), dynamic model discovery (`/v1/models`, `/api/tags`), and SSE streaming
 * responses across OpenAI Responses, OpenAI Chat Completions, Anthropic Messages, and Gemini
 * `streamGenerateContent` without external API calls or billable tokens.
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as http from 'http';
import * as fs from 'fs';
import * as path from 'path';

export interface RecordedInferenceRequest {
  method: string;
  url: string;
  headers: Record<string, string | string[] | undefined>;
  bodyText: string;
  bodyJson?: Record<string, unknown>;
  authorized: boolean;
}

export type ScriptedTurnHandler = (
  req: RecordedInferenceRequest,
  turnIndex: number
) => {
  status?: number;
  text?: string;
  toolCall?: {
    id: string;
    name: string;
    argumentsJson: string;
    fragmentArguments?: boolean;
  };
  errorBody?: Record<string, unknown>;
};

export interface DeterministicServerOptions {
  expectedBearerToken?: string;
  expectedAnthropicKey?: string;
  expectedGeminiKey?: string;
  models?: Array<{ id: string; context_window?: number }>;
  onInference?: ScriptedTurnHandler;
  streamDelayMs?: number;
}

export class DeterministicInferenceServer {
  private server: http.Server | null = null;
  private port: number = 0;
  private inferenceCount: number = 0;
  public readonly requests: RecordedInferenceRequest[] = [];

  constructor(private options: DeterministicServerOptions = {}) {}

  public setOptions(next: Partial<DeterministicServerOptions>): void {
    this.options = { ...this.options, ...next };
  }

  public get url(): string {
    return `http://127.0.0.1:${this.port}`;
  }

  public get v1Url(): string {
    return `${this.url}/v1`;
  }

  public get geminiBaseUrl(): string {
    return `${this.url}/v1beta`;
  }

  public async start(): Promise<string> {
    if (this.server) {
      return this.url;
    }

    this.server = http.createServer((req, res) => {
      const chunks: Buffer[] = [];
      req.on('data', (c: Buffer) => chunks.push(c));
      req.on('end', () => {
        const bodyText = Buffer.concat(chunks).toString('utf8');
        let bodyJson: Record<string, unknown> | undefined;
        try {
          bodyJson = bodyText ? (JSON.parse(bodyText) as Record<string, unknown>) : undefined;
        } catch {
          bodyJson = undefined;
        }

        const urlPath = req.url || '/';
        const isAuthorized = this.checkAuth(urlPath, req.headers);
        const rec: RecordedInferenceRequest = {
          method: req.method || 'GET',
          url: urlPath,
          headers: req.headers,
          bodyText,
          bodyJson,
          authorized: isAuthorized,
        };
        this.requests.push(rec);

        if (!isAuthorized) {
          res.writeHead(401, { 'Content-Type': 'application/json' });
          res.end(
            JSON.stringify({
              error: {
                message: 'Invalid or missing API credential',
                type: 'authentication_error',
                code: 'invalid_api_key',
              },
            })
          );
          return;
        }

        void this.handleRoute(rec, res);
      });
    });

    await new Promise<void>((resolve, reject) => {
      this.server!.once('error', reject);
      this.server!.listen(0, '127.0.0.1', () => {
        const addr = this.server!.address();
        if (addr && typeof addr === 'object') {
          this.port = addr.port;
        }
        resolve();
      });
    });

    return this.url;
  }

  public async stop(): Promise<void> {
    if (!this.server) {
      return;
    }
    const srv = this.server;
    this.server = null;
    await new Promise<void>((resolve) => {
      srv.close(() => resolve());
    });
  }

  private checkAuth(
    urlPath: string,
    headers: Record<string, string | string[] | undefined>
  ): boolean {
    if (urlPath.includes('/messages')) {
      if (!this.options.expectedAnthropicKey) {
        return true;
      }
      return headers['x-api-key'] === this.options.expectedAnthropicKey;
    }
    if (urlPath.includes('generateContent') || urlPath.includes('/v1beta/')) {
      if (!this.options.expectedGeminiKey) {
        return true;
      }
      return headers['x-goog-api-key'] === this.options.expectedGeminiKey;
    }
    if (this.options.expectedBearerToken) {
      return headers.authorization === `Bearer ${this.options.expectedBearerToken}`;
    }
    return true;
  }

  private async handleRoute(rec: RecordedInferenceRequest, res: http.ServerResponse): Promise<void> {
    const models = this.options.models ?? [
      { id: 'codex-test-model', context_window: 128000 },
      { id: 'gpt-5-codex', context_window: 200000 },
    ];

    if (rec.method === 'GET' && rec.url.endsWith('/models')) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(
        JSON.stringify({
          object: 'list',
          data: models.map((m) => ({
            id: m.id,
            object: 'model',
            owned_by: 'deterministic-fixture',
            context_window: m.context_window ?? 128000,
          })),
        })
      );
      return;
    }

    if (rec.method === 'GET' && rec.url.endsWith('/api/tags')) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(
        JSON.stringify({
          models: models.map((m) => ({ name: m.id, id: m.id })),
        })
      );
      return;
    }

    const turnIdx = ++this.inferenceCount;
    const outcome = this.options.onInference
      ? this.options.onInference(rec, turnIdx)
      : { text: `Deterministic response turn #${turnIdx}` };

    if (outcome.status && outcome.status >= 400) {
      res.writeHead(outcome.status, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(outcome.errorBody ?? { error: { message: 'Injected HTTP error' } }));
      return;
    }

    if (this.options.streamDelayMs && this.options.streamDelayMs > 0) {
      await new Promise((r) => setTimeout(r, this.options.streamDelayMs));
    }

    res.writeHead(200, {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
      Connection: 'keep-alive',
    });

    if (rec.url.includes('/messages')) {
      this.writeAnthropicSse(res, outcome.text ?? 'Anthropic deterministic reply');
      return;
    }

    if (rec.url.includes('streamGenerateContent') || rec.url.includes('/v1beta/')) {
      this.writeGeminiSse(res, outcome.text ?? 'Gemini deterministic reply');
      return;
    }

    if (rec.url.includes('/responses')) {
      this.writeOpenAiResponsesSse(res, outcome);
      return;
    }

    this.writeOpenAiChatCompletionsSse(res, outcome);
  }

  private writeOpenAiChatCompletionsSse(
    res: http.ServerResponse,
    outcome: ReturnType<ScriptedTurnHandler>
  ): void {
    const id = `chatcmpl_${Date.now()}`;
    if (outcome.toolCall) {
      const args = outcome.toolCall.argumentsJson;
      const chunks = outcome.toolCall.fragmentArguments
        ? [args.slice(0, Math.ceil(args.length / 2)), args.slice(Math.ceil(args.length / 2))]
        : [args];

      res.write(
        `data: ${JSON.stringify({
          id,
          choices: [
            {
              index: 0,
              delta: {
                role: 'assistant',
                tool_calls: [
                  {
                    index: 0,
                    id: outcome.toolCall.id,
                    type: 'function',
                    function: { name: outcome.toolCall.name, arguments: chunks[0] },
                  },
                ],
              },
              finish_reason: null,
            },
          ],
        })}\n\n`
      );
      if (chunks[1]) {
        res.write(
          `data: ${JSON.stringify({
            id,
            choices: [
              {
                index: 0,
                delta: {
                  tool_calls: [{ index: 0, function: { arguments: chunks[1] } }],
                },
                finish_reason: null,
              },
            ],
          })}\n\n`
        );
      }
      res.write(
        `data: ${JSON.stringify({
          id,
          choices: [{ index: 0, delta: {}, finish_reason: 'tool_calls' }],
          usage: { prompt_tokens: 18, completion_tokens: 12, total_tokens: 30 },
        })}\n\n`
      );
      res.write('data: [DONE]\n\n');
      res.end();
      return;
    }

    const fullText = outcome.text ?? '';
    const mid = Math.ceil(fullText.length / 2);
    const part1 = fullText.slice(0, mid);
    const part2 = fullText.slice(mid);
    if (part1) {
      res.write(
        `data: ${JSON.stringify({
          id,
          choices: [{ index: 0, delta: { role: 'assistant', content: part1 }, finish_reason: null }],
        })}\n\n`
      );
    }
    if (part2) {
      res.write(
        `data: ${JSON.stringify({
          id,
          choices: [{ index: 0, delta: { content: part2 }, finish_reason: null }],
        })}\n\n`
      );
    }
    res.write(
      `data: ${JSON.stringify({
        id,
        choices: [{ index: 0, delta: {}, finish_reason: 'stop' }],
        usage: { prompt_tokens: 20, completion_tokens: 15, total_tokens: 35 },
      })}\n\n`
    );
    res.write('data: [DONE]\n\n');
    res.end();
  }

  private writeOpenAiResponsesSse(
    res: http.ServerResponse,
    outcome: ReturnType<ScriptedTurnHandler>
  ): void {
    const respId = `resp_${Date.now()}`;
    const itemId = `msg_${Date.now()}`;
    res.write(`event: response.created\ndata: ${JSON.stringify({ type: 'response.created', response: { id: respId } })}\n\n`);

    if (outcome.toolCall) {
      res.write(
        `event: response.output_item.done\ndata: ${JSON.stringify({
          type: 'response.output_item.done',
          output_index: 0,
          item: {
            type: 'function_call',
            id: `fc_${outcome.toolCall.id}`,
            call_id: outcome.toolCall.id,
            name: outcome.toolCall.name,
            arguments: outcome.toolCall.argumentsJson,
          },
        })}\n\n`
      );
    } else {
      const fullText = outcome.text ?? '';
      res.write(
        `event: response.output_item.added\ndata: ${JSON.stringify({
          type: 'response.output_item.added',
          output_index: 0,
          item: { type: 'message', id: itemId, role: 'assistant', content: [] },
        })}\n\n`
      );
      res.write(
        `event: response.output_text.delta\ndata: ${JSON.stringify({
          type: 'response.output_text.delta',
          item_id: itemId,
          output_index: 0,
          content_index: 0,
          delta: fullText,
        })}\n\n`
      );
      res.write(
        `event: response.output_item.done\ndata: ${JSON.stringify({
          type: 'response.output_item.done',
          output_index: 0,
          item: {
            type: 'message',
            id: itemId,
            role: 'assistant',
            content: [{ type: 'output_text', text: fullText }],
          },
        })}\n\n`
      );
    }

    res.write(
      `event: response.completed\ndata: ${JSON.stringify({
        type: 'response.completed',
        response: {
          id: respId,
          usage: { input_tokens: 15, output_tokens: 10, total_tokens: 25 },
        },
      })}\n\n`
    );
    res.end();
  }

  private writeAnthropicSse(res: http.ServerResponse, text: string): void {
    res.write(
      `event: message_start\ndata: ${JSON.stringify({
        type: 'message_start',
        message: {
          id: `msg_${Date.now()}`,
          type: 'message',
          role: 'assistant',
          content: [],
          usage: { input_tokens: 14, output_tokens: 1 },
        },
      })}\n\n`
    );
    res.write(
      `event: content_block_start\ndata: ${JSON.stringify({
        type: 'content_block_start',
        index: 0,
        content_block: { type: 'text', text: '' },
      })}\n\n`
    );
    res.write(
      `event: content_block_delta\ndata: ${JSON.stringify({
        type: 'content_block_delta',
        index: 0,
        delta: { type: 'text_delta', text },
      })}\n\n`
    );
    res.write(`event: content_block_stop\ndata: ${JSON.stringify({ type: 'content_block_stop', index: 0 })}\n\n`);
    res.write(
      `event: message_delta\ndata: ${JSON.stringify({
        type: 'message_delta',
        delta: { stop_reason: 'end_turn' },
        usage: { output_tokens: 9 },
      })}\n\n`
    );
    res.write(`event: message_stop\ndata: ${JSON.stringify({ type: 'message_stop' })}\n\n`);
    res.end();
  }

  private writeGeminiSse(res: http.ServerResponse, text: string): void {
    res.write(
      `data: ${JSON.stringify({
        candidates: [
          {
            content: { role: 'model', parts: [{ text }] },
            finishReason: 'STOP',
            index: 0,
          },
        ],
        usageMetadata: { promptTokenCount: 11, candidatesTokenCount: 8, totalTokenCount: 19 },
      })}\n\n`
    );
    res.end();
  }
}

export function initTempGitRepo(dir: string): void {
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'README.md'), '# Temp Test Repo\n', 'utf8');
  fs.writeFileSync(path.join(dir, 'app.ts'), 'export function greet(): string { return "hello"; }\n', 'utf8');
}
