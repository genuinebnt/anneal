// Loads editors/vscode/extension.js with fake `vscode` and `vscode-languageclient`
// modules (nothing is installed from npm), activates it, and checks that the client
// is built with the right command and arguments, started, and stopped. The fake
// client really spawns the command so that the command line is proven to work.
// Usage: node mock.js EXTENSION_JS SERVER_PATH
'use strict';
const Module = require('module');
const assert = require('assert');
const { spawn } = require('child_process');
const [extensionPath, serverPath] = process.argv.slice(2);

const configReads = [];
const clients = [];
class LanguageClient {
  constructor(id, name, serverOptions, clientOptions) {
    Object.assign(this, { id, name, serverOptions, clientOptions, started: 0, stopped: 0 });
    clients.push(this);
  }
  start() {
    this.started += 1;
    this.child = spawn(this.serverOptions.command, this.serverOptions.args, { stdio: ['pipe', 'pipe', 'inherit'] });
    return Promise.resolve();
  }
  stop() {
    this.stopped += 1;
    return Promise.resolve();
  }
}
const fakes = {
  vscode: {
    workspace: {
      getConfiguration(section) {
        return {
          get(key, fallback) {
            configReads.push(`${section}.${key}`);
            assert.strictEqual(fallback, 'pebble-tools', 'default server command');
            return serverPath;
          },
        };
      },
    },
  },
  'vscode-languageclient/node': { LanguageClient, TransportKind: { stdio: 0 } },
};
const load = Module._load;
Module._load = function (request, ...rest) {
  return Object.prototype.hasOwnProperty.call(fakes, request) ? fakes[request] : load.call(this, request, ...rest);
};

const extension = require(extensionPath);
assert.strictEqual(extension.deactivate(), undefined, 'deactivate before activate is a no-op');
extension.activate({ subscriptions: [] });
assert.deepStrictEqual(configReads, ['pebble.server.path']);
assert.strictEqual(clients.length, 1);
const [client] = clients;
assert.strictEqual(client.started, 1);
assert.strictEqual(client.id, 'pebble');
assert.strictEqual(client.serverOptions.command, serverPath);
assert.deepStrictEqual(client.serverOptions.args, ['lsp']);
assert.strictEqual(client.serverOptions.transport, 0, 'stdio transport');
assert.deepStrictEqual(client.clientOptions.documentSelector, [{ scheme: 'file', language: 'pebble' }]);

const body = JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { capabilities: {} } });
let output = '';
const timer = setTimeout(() => {
  client.child.kill('SIGKILL');
  console.error('timed out waiting for the server');
  process.exit(1);
}, 20000);
client.child.stdout.on('data', (chunk) => {
  output += chunk;
  if (output.includes('"serverInfo"')) {
    clearTimeout(timer);
    client.child.kill();
    extension.deactivate().then(() => {
      assert.strictEqual(client.stopped, 1);
      console.log('mock client ok');
      process.exit(0);
    });
  }
});
client.child.stdin.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
