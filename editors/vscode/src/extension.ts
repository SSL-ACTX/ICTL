import * as path from 'path';
import { workspace, ExtensionContext } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    Executable
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
    const config = workspace.getConfiguration('ictl');
    let lspPath = config.get<string>('lsp.path');

    if (!lspPath) {
        // Fallback to local development path if not configured
        lspPath = context.asAbsolutePath(path.join('..', '..', 'target', 'debug', 'ictl-lsp'));
    }

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
