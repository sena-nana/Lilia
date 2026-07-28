// #47 LEGACY — Codex account quota via official app-server (source-only; not default-bundled).
import {
  consumeCodexRateLimitResetCredit,
  readCodexAccountQuotaStatus,
} from "./agent-runner/codex/accountQuota.mjs";

function writeJson(payload) {
  process.stdout.write(`${JSON.stringify(payload)}\n`);
}

async function main() {
  const [command, value] = process.argv.slice(2);
  if (command === "--consume-reset-credit") {
    writeJson(await consumeCodexRateLimitResetCredit(value));
    return;
  }
  writeJson(await readCodexAccountQuotaStatus());
}

main().catch((err) => {
  if (process.argv[2] === "--consume-reset-credit") {
    process.stderr.write(`${err?.message || String(err)}\n`);
    process.exit(1);
  }
  writeJson({
    available: false,
    connectionMode: "codex-account",
    limitId: null,
    limitName: null,
    planType: null,
    rateLimitReachedType: null,
    fiveHour: null,
    weekly: null,
    sparkFiveHour: null,
    sparkWeekly: null,
    credits: null,
    sparkCredits: null,
    rateLimitResetCredits: null,
    accountUsage: null,
    usageError: null,
    fetchedAt: Date.now(),
    error: err?.message || String(err),
  });
  process.exit(1);
});
