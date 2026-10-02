// Runs Python 3 with the given arguments, so npm scripts work everywhere: Windows installs
// `python` (and `py`), not `python3`.
import { spawnSync } from "node:child_process";

const python = process.platform === "win32" ? "python" : "python3";
const { status, error } = spawnSync(python, process.argv.slice(2), { stdio: "inherit" });
if (error) console.error(`${python}: ${error.message}`);
process.exit(status ?? 1);
