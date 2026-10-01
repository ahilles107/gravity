import { appendFileSync } from "node:fs";

appendFileSync("../pty-launches", "started\n");
process.stdout.write("ready-for-runtime-change\n");
process.stdin.resume();
setInterval(() => {}, 1000);
