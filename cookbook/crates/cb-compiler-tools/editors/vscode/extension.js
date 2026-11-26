// Thin LSP client: starts `pebble-tools lsp` over stdio. Install dependencies with `npm install`.
const { workspace } = require('vscode');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;

function activate() {
  const command = workspace.getConfiguration('pebble').get('server.path', 'pebble-tools');
  const serverOptions = { command, args: ['lsp'], transport: TransportKind.stdio };
  const clientOptions = { documentSelector: [{ scheme: 'file', language: 'pebble' }] };
  client = new LanguageClient('pebble', 'PEBBLE Language Server', serverOptions, clientOptions);
  client.start();
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
