"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.deactivate = exports.activate = void 0;
const path = require("path");
const vscode_1 = require("vscode");
const node_1 = require("vscode-languageclient/node");
let client;
function activate(context) {
    const config = vscode_1.workspace.getConfiguration('ictl');
    let lspPath = config.get('lsp.path');
    if (!lspPath) {
        // Fallback to local development path if not configured
        lspPath = context.asAbsolutePath(path.join('..', '..', 'target', 'debug', 'ictl-lsp'));
    }
    const serverOptions = {
        run: { command: lspPath },
        debug: { command: lspPath }
    };
    const clientOptions = {
        documentSelector: [{ scheme: 'file', language: 'ictl' }],
        synchronize: {
            fileEvents: vscode_1.workspace.createFileSystemWatcher('**/*.ictl')
        }
    };
    client = new node_1.LanguageClient('ictlLanguageServer', 'ICTL Language Server', serverOptions, clientOptions);
    client.start();
}
exports.activate = activate;
function deactivate() {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
exports.deactivate = deactivate;
//# sourceMappingURL=extension.js.map