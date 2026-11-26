-- Headless Neovim smoke test for `pebble-tools lsp` (Neovim's own LSP client).
--
--   PEBBLE_TOOLS=/path/to/pebble-tools nvim --headless -u NONE -i NONE -l scripts/nvim_smoke.lua
--
-- Exits 0 only if every step passed.

local server = os.getenv('PEBBLE_TOOLS') or 'pebble-tools'
local dir = vim.fn.tempname()
vim.fn.mkdir(dir, 'p')
local path = dir .. '/smoke.peb'
local source = {
  '/// Adds two numbers.',
  'fn add(a: int, b: int) -> int { a + b }',
  'fn main() {',
  '    let bad: int = "x";',
  '    println(to_str(add(1, 2)));',
  '}',
}
vim.fn.writefile(source, path)

local function fail(msg)
  io.stdout:write('FAIL ' .. msg .. '\n')
  vim.cmd('cquit 1')
end

local function ok(label, detail)
  io.stdout:write('ok   ' .. label .. ': ' .. detail .. '\n')
end

vim.cmd('edit ' .. vim.fn.fnameescape(path))
local buf = vim.api.nvim_get_current_buf()
vim.bo[buf].filetype = 'pebble'
local id = vim.lsp.start({ name = 'pebble', cmd = { server, 'lsp' }, root_dir = dir })
if not id then fail('vim.lsp.start returned nil') end
local client = vim.lsp.get_client_by_id(id)
if not vim.wait(10000, function() return client.initialized == true end, 20) then fail('initialize timed out') end
ok('initialize', client.server_info.name .. ' ' .. client.server_info.version)

if not vim.wait(10000, function() return #vim.diagnostic.get(buf) > 0 end, 20) then fail('no diagnostics') end
local errors = vim.diagnostic.get(buf, { severity = vim.diagnostic.severity.ERROR })
if #errors == 0 or errors[1].lnum ~= 3 then fail('expected an error on line 4') end
ok('diagnostics', string.format('line %d %s %s', errors[1].lnum + 1, tostring(errors[1].code), (errors[1].message:gsub('\n.*', ''))))

local function request(method, params)
  local res, err = client:request_sync(method, params, 10000, buf)
  if err then fail(method .. ': ' .. tostring(err)) end
  if res.err then fail(method .. ': ' .. res.err.message) end
  return res.result
end

local doc = { uri = vim.uri_from_bufnr(buf) }
local col = source[5]:find('add') - 1
local at = { textDocument = doc, position = { line = 4, character = col } }

local hover = request('textDocument/hover', at)
if not hover.contents.value:find('fn add') then fail('hover: ' .. vim.inspect(hover)) end
ok('hover', (hover.contents.value:gsub('\n', ' | ')))

local def = request('textDocument/definition', at)
local range = def.range or def[1].range
if range.start.line ~= 1 then fail('definition: ' .. vim.inspect(def)) end
ok('definition', 'line ' .. (range.start.line + 1))

local symbols = request('workspace/symbol', { query = 'ad' })
if not symbols[1] or symbols[1].name ~= 'add' then fail('workspace/symbol: ' .. vim.inspect(symbols)) end
ok('workspace/symbol', symbols[1].name)

local rename = request('textDocument/rename', vim.tbl_extend('force', at, { newName = 'plus' }))
vim.lsp.util.apply_workspace_edit(rename, client.offset_encoding)
local text = table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), '\n')
if not (text:find('fn plus') and text:find('to_str%(plus%(1, 2%)%)')) then fail('rename:\n' .. text) end
ok('rename', 'both occurrences rewritten by the client')

-- The client sends the edit as a didChange; formatting then works on the new text.
local edits = request('textDocument/formatting', { textDocument = doc, options = { tabSize = 4, insertSpaces = true } })
vim.lsp.util.apply_text_edits(edits, buf, client.offset_encoding)
text = table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), '\n')
if not text:find('fn plus%(a: int, b: int%) %-> int {\n    a %+ b\n}') then fail('formatting:\n' .. text) end
ok('formatting', #edits .. ' edit(s) applied by the client')

client:stop()
if not vim.wait(10000, function() return client:is_stopped() end, 20) then fail('shutdown timed out') end
ok('shutdown', 'client stopped')
io.stdout:write('all steps passed\n')
vim.cmd('qall!')
