local cmd = require("cmd")
local json = require("json")

function PLUGIN:MiseEnv(ctx)
  local cwd = os.getenv("PWD")
  if not cwd then
    return {}
  end
  local quoted_cwd = "'" .. cwd:gsub("'", "'\\''") .. "'"
  local ok, result = pcall(cmd.exec, "grove env export --json " .. quoted_cwd)
  if not ok or result == "" or result == "{}" then
    return {}
  end
  local vars = json.decode(result)
  local env = {}
  for key, value in pairs(vars) do
    table.insert(env, {key = key, value = value})
  end
  return env
end
