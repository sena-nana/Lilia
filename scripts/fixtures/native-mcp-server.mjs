import fs from "node:fs";

const markerPath = process.argv[2];
const credentialPresent = Boolean(process.env.NATIVE_DEBUG_TOKEN);
let buffered = Buffer.alloc(0);

process.stdin.on("data", (chunk) => {
  buffered = Buffer.concat([buffered, chunk]);
  drainFrames();
});

function drainFrames() {
  for (;;) {
    const headerEnd = buffered.indexOf("\r\n\r\n");
    if (headerEnd < 0) return;
    const header = buffered.subarray(0, headerEnd).toString("ascii");
    const match = /^Content-Length:\s*(\d+)$/im.exec(header);
    if (!match) process.exit(2);
    const length = Number(match[1]);
    const bodyStart = headerEnd + 4;
    if (!Number.isSafeInteger(length) || length < 0 || buffered.length < bodyStart + length) {
      return;
    }
    const body = buffered.subarray(bodyStart, bodyStart + length);
    buffered = buffered.subarray(bodyStart + length);
    handleMessage(JSON.parse(body.toString("utf8")));
  }
}

function handleMessage(message) {
  const { id, method, params = {} } = message;
  if (id === undefined || id === null) return;
  if (method === "initialize") {
    if (!credentialPresent) {
      respond(id, undefined, { code: -32001, message: "fixture credential is unavailable" });
      return;
    }
    respond(id, {
      protocolVersion: "2024-11-05",
      capabilities: { tools: {}, resources: {}, prompts: {} },
      serverInfo: { name: "lilia-native-debug-mcp", version: "1.0.0" },
    });
    return;
  }
  if (method === "tools/list") {
    respond(id, {
      tools: [
        {
          name: "credential_probe",
          description: "Verifies that the Native host injected an OS Keyring credential.",
          inputSchema: {
            type: "object",
            properties: { text: { type: "string" } },
            required: ["text"],
          },
          annotations: {
            readOnlyHint: true,
            destructiveHint: false,
            idempotentHint: true,
            openWorldHint: false,
          },
        },
      ],
    });
    return;
  }
  if (method === "resources/list") {
    respond(id, {
      resources: [
        {
          uri: "mcp://native-debug/credential-status",
          name: "Native credential status",
          description: "Secret-free credential injection status.",
          mimeType: "application/json",
        },
      ],
    });
    return;
  }
  if (method === "prompts/list") {
    respond(id, {
      prompts: [
        {
          name: "credential_summary",
          description: "Summarizes the Native credential probe.",
          arguments: [
            { name: "scope", description: "Probe scope", required: true },
          ],
        },
      ],
    });
    return;
  }
  if (method === "tools/call" && params.name === "credential_probe") {
    const text = typeof params.arguments?.text === "string" ? params.arguments.text : "";
    if (markerPath) {
      fs.writeFileSync(
        markerPath,
        `${JSON.stringify({ called: true, credentialPresent, text })}\n`,
        "utf8",
      );
    }
    respond(id, {
      content: [
        {
          type: "text",
          text: JSON.stringify({ credentialPresent, echo: text }),
        },
      ],
      isError: false,
    });
    return;
  }
  if (method === "resources/read") {
    respond(id, {
      contents: [
        {
          uri: params.uri,
          mimeType: "application/json",
          text: JSON.stringify({ credentialPresent }),
        },
      ],
    });
    return;
  }
  if (method === "prompts/get") {
    respond(id, {
      messages: [
        {
          role: "user",
          content: {
            type: "text",
            text: `Native credential scope: ${params.arguments?.scope ?? "unknown"}`,
          },
        },
      ],
    });
    return;
  }
  respond(id, undefined, { code: -32601, message: `unsupported fixture method ${method}` });
}

function respond(id, result, error) {
  const body = Buffer.from(
    JSON.stringify({ jsonrpc: "2.0", id, ...(error ? { error } : { result }) }),
    "utf8",
  );
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
  process.stdout.write(body);
}
