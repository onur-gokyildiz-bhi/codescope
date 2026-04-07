import * as vscode from 'vscode';
import { ChildProcess, spawn } from 'child_process';

// ---------------------------------------------------------------------------
// MCP Client — manages a codescope-mcp child process over stdio JSON-RPC
// ---------------------------------------------------------------------------

class CodescopeMcpClient {
    private process: ChildProcess | null = null;
    private nextId = 1;
    private pending = new Map<number, { resolve: (v: any) => void; reject: (e: Error) => void }>();
    private buffer = '';

    start(workspacePath: string): void {
        if (this.process) {
            return;
        }

        this.process = spawn('codescope', ['mcp', workspacePath, '--auto-index'], {
            stdio: ['pipe', 'pipe', 'pipe'],
        });

        this.process.stdout?.on('data', (chunk: Buffer) => {
            this.buffer += chunk.toString();
            this.drain();
        });

        this.process.stderr?.on('data', (chunk: Buffer) => {
            const msg = chunk.toString().trim();
            if (msg) {
                console.error('[codescope-mcp]', msg);
            }
        });

        this.process.on('exit', (code) => {
            console.log(`[codescope-mcp] exited with code ${code}`);
            this.process = null;
            // Reject all pending requests
            for (const [, p] of this.pending) {
                p.reject(new Error(`codescope-mcp exited with code ${code}`));
            }
            this.pending.clear();
        });
    }

    stop(): void {
        this.process?.kill();
        this.process = null;
    }

    get isRunning(): boolean {
        return this.process !== null;
    }

    /** Send a JSON-RPC request and await the response. */
    async call(method: string, params: Record<string, unknown> = {}): Promise<any> {
        if (!this.process?.stdin) {
            throw new Error('codescope-mcp is not running');
        }

        const id = this.nextId++;
        const request = JSON.stringify({ jsonrpc: '2.0', id, method, params });
        this.process.stdin.write(request + '\n');

        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject });
        });
    }

    /** Parse newline-delimited JSON-RPC responses from the buffer. */
    private drain(): void {
        const lines = this.buffer.split('\n');
        this.buffer = lines.pop() ?? '';

        for (const line of lines) {
            const trimmed = line.trim();
            if (!trimmed) {
                continue;
            }
            try {
                const msg = JSON.parse(trimmed);
                if (msg.id != null && this.pending.has(msg.id)) {
                    const p = this.pending.get(msg.id)!;
                    this.pending.delete(msg.id);
                    if (msg.error) {
                        p.reject(new Error(msg.error.message ?? JSON.stringify(msg.error)));
                    } else {
                        p.resolve(msg.result);
                    }
                }
            } catch {
                // Ignore malformed lines
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CodeLens provider — placeholder that shows "codescope" above functions
// ---------------------------------------------------------------------------

class CodescopeCodeLensProvider implements vscode.CodeLensProvider {
    provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
        const lenses: vscode.CodeLens[] = [];
        const fnPattern = /^\s*(?:pub\s+)?(?:async\s+)?fn\s+\w+|(?:export\s+)?(?:async\s+)?function\s+\w+|(?:def|class)\s+\w+/;

        for (let i = 0; i < document.lineCount; i++) {
            const line = document.lineAt(i);
            if (fnPattern.test(line.text)) {
                const range = new vscode.Range(i, 0, i, line.text.length);
                lenses.push(
                    new vscode.CodeLens(range, {
                        title: 'codescope',
                        command: '',
                        tooltip: 'Codescope: caller/callee info will appear here',
                    })
                );
            }
        }

        return lenses;
    }
}

// ---------------------------------------------------------------------------
// Extension activation
// ---------------------------------------------------------------------------

let client: CodescopeMcpClient;

export function activate(context: vscode.ExtensionContext): void {
    client = new CodescopeMcpClient();

    // Start the MCP server for the first workspace folder
    const workspacePath = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (workspacePath) {
        client.start(workspacePath);
    }

    // Register CodeLens provider for all languages
    context.subscriptions.push(
        vscode.languages.registerCodeLensProvider({ scheme: 'file' }, new CodescopeCodeLensProvider())
    );

    // --- Commands ---

    context.subscriptions.push(
        vscode.commands.registerCommand('codescope.index', async () => {
            const folder = workspacePath ?? await pickFolder();
            if (!folder) {
                return;
            }

            if (!client.isRunning) {
                client.start(folder);
            }

            await vscode.window.withProgress(
                { location: vscode.ProgressLocation.Notification, title: 'Codescope: Indexing...' },
                async () => {
                    try {
                        const result = await client.call('tools/call', {
                            name: 'index_project',
                            arguments: { path: folder },
                        });
                        vscode.window.showInformationMessage(`Codescope: Indexing complete — ${JSON.stringify(result)}`);
                    } catch (err: any) {
                        vscode.window.showErrorMessage(`Codescope index failed: ${err.message}`);
                    }
                }
            );
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('codescope.search', async () => {
            const query = await vscode.window.showInputBox({
                prompt: 'Search functions by name',
                placeHolder: 'e.g. parse_config',
            });
            if (!query) {
                return;
            }

            try {
                const result = await client.call('tools/call', {
                    name: 'search_functions',
                    arguments: { pattern: query },
                });
                const output = vscode.window.createOutputChannel('Codescope');
                output.clear();
                output.appendLine(JSON.stringify(result, null, 2));
                output.show();
            } catch (err: any) {
                vscode.window.showErrorMessage(`Codescope search failed: ${err.message}`);
            }
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('codescope.stats', async () => {
            try {
                const result = await client.call('tools/call', {
                    name: 'stats',
                    arguments: {},
                });
                const output = vscode.window.createOutputChannel('Codescope');
                output.clear();
                output.appendLine(JSON.stringify(result, null, 2));
                output.show();
            } catch (err: any) {
                vscode.window.showErrorMessage(`Codescope stats failed: ${err.message}`);
            }
        })
    );
}

export function deactivate(): void {
    client?.stop();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function pickFolder(): Promise<string | undefined> {
    const uri = await vscode.window.showOpenDialog({
        canSelectFolders: true,
        canSelectFiles: false,
        openLabel: 'Select project folder',
    });
    return uri?.[0]?.fsPath;
}
