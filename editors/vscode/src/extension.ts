import * as fs from 'fs';
import * as path from 'path';
import { workspace, window, ExtensionContext } from 'vscode';
import { execSync } from 'child_process';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    Executable
} from 'vscode-languageclient/node';

let client: LanguageClient;

function isExecutable(filePath: string): boolean {
    try {
        const stat = fs.statSync(filePath);
        return stat.isFile();
    } catch {
        return false;
    }
}

function resolveViaWhich(command: string): string | undefined {
    const resolver = process.platform === 'win32' ? 'where' : 'which';
    try {
        const stdout = execSync(`${resolver} ${command}`, {
            stdio: ['ignore', 'pipe', 'ignore'],
            encoding: 'utf8',
            timeout: 5000
        });
        for (const line of stdout.split(/\r?\n/)) {
            const candidate = line.trim();
            if (candidate && isExecutable(candidate)) {
                return candidate;
            }
        }
    } catch {
        // ignore missing command or error
    }
    return undefined;
}

function resolveIctlLspPath(context: ExtensionContext): string | undefined {
    const config = workspace.getConfiguration('ictl');
    const configPath = config.get<string>('lsp.path');
    const executableName = process.platform === 'win32' ? 'ictl-lsp.exe' : 'ictl-lsp';

    if (configPath) {
        if (path.isAbsolute(configPath) && isExecutable(configPath)) {
            return configPath;
        }
        return configPath;
    }

    // Try PATH lookup first.
    const fromPath = resolveViaWhich(executableName);
    if (fromPath) {
        return fromPath;
    }

    const candidates: string[] = [];
    if (workspace.workspaceFolders) {
        for (const folder of workspace.workspaceFolders) {
            candidates.push(path.join(folder.uri.fsPath, 'target', 'debug', executableName));
            candidates.push(path.join(folder.uri.fsPath, 'target', 'release', executableName));
            candidates.push(path.join(folder.uri.fsPath, 'target', executableName));
        }
    }

    const extensionPath = context.extensionUri.fsPath;
    candidates.push(path.join(extensionPath, '..', '..', 'target', 'debug', executableName));
    candidates.push(path.join(extensionPath, '..', '..', 'target', 'release', executableName));

    candidates.push(path.join(process.cwd(), 'target', 'debug', executableName));
    candidates.push(path.join(process.cwd(), 'target', 'release', executableName));

    for (const candidate of candidates) {
        if (candidate && isExecutable(candidate)) {
            return candidate;
        }
    }

    // No valid binary found.
    return undefined;
}

export function activate(context: ExtensionContext) {
    const lspPath = resolveIctlLspPath(context);

    if (!lspPath) {
        const msg =
            'Could not resolve ictl-lsp path (looked for setting, workspace targets, extension path, and PATH).\n' +
            'Run `cargo build --bin ictl-lsp` and set `ictl.lsp.path` explicitly if needed.';
        window.showErrorMessage(msg);
        console.error(msg);
        return;
    }

    window.showInformationMessage(`Using ictl-lsp binary: ${lspPath}`);

    const serverOptions: ServerOptions = {
        run: { command: lspPath },
        debug: { command: lspPath }
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'ictl' }],
        synchronize: {
            fileEvents: workspace.createFileSystemWatcher('**/*.ictl')
        }
    };

    client = new LanguageClient(
        'ictlLanguageServer',
        'ICTL Language Server',
        serverOptions,
        clientOptions
    );

    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
