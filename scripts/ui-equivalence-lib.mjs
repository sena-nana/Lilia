import assert from "node:assert/strict";
import net from "node:net";

export function firstDifference(left, right, location = "snapshot") {
  if (Object.is(left, right)) return null;
  if (Array.isArray(left) || Array.isArray(right)) {
    if (!Array.isArray(left) || !Array.isArray(right)) {
      return { location, left, right };
    }
    if (left.length !== right.length) {
      return {
        location: `${location}.length`,
        left: left.length,
        right: right.length,
      };
    }
    for (let index = 0; index < left.length; index += 1) {
      const difference = firstDifference(left[index], right[index], `${location}[${index}]`);
      if (difference) return difference;
    }
    return null;
  }
  if (left && right && typeof left === "object" && typeof right === "object") {
    const keys = [...new Set([...Object.keys(left), ...Object.keys(right)])].sort();
    for (const key of keys) {
      if (!Object.hasOwn(left, key) || !Object.hasOwn(right, key)) {
        return { location: `${location}.${key}`, left: left[key], right: right[key] };
      }
      const difference = firstDifference(left[key], right[key], `${location}.${key}`);
      if (difference) return difference;
    }
    return null;
  }
  return { location, left, right };
}

export function assertEquivalent(left, right, label) {
  const difference = firstDifference(left, right);
  if (difference) {
    assert.fail(
      `${label} differs at ${difference.location}: ` +
        `${JSON.stringify(difference.left)} !== ${JSON.stringify(difference.right)}`,
    );
  }
}

export function requestNativeDebug(address, command, timeoutMs = 15_000) {
  return new Promise((resolve, reject) => {
    const separator = address.lastIndexOf(":");
    const host = address.slice(0, separator);
    const port = Number.parseInt(address.slice(separator + 1), 10);
    if (!host || !Number.isInteger(port)) {
      reject(new Error(`Invalid Native debug address: ${address}`));
      return;
    }
    const socket = net.createConnection({ host, port });
    let response = "";
    const timeout = setTimeout(() => {
      socket.destroy(new Error(`Native debug command ${command.command} timed out`));
    }, timeoutMs);
    socket.setEncoding("utf8");
    socket.once("connect", () => socket.write(`${JSON.stringify(command)}\n`));
    socket.on("data", (chunk) => {
      response += chunk;
      const newline = response.indexOf("\n");
      if (newline < 0) return;
      clearTimeout(timeout);
      socket.end();
      try {
        const parsed = JSON.parse(response.slice(0, newline));
        if (!parsed.ok) {
          reject(new Error(parsed.error?.message ?? `Native debug command ${command.command} failed`));
          return;
        }
        resolve(parsed);
      } catch (error) {
        reject(error);
      }
    });
    socket.once("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
  });
}

export async function waitUntil(label, timeoutMs, check, intervalMs = 250) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const result = await check();
      if (result) return result;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new Error(
    `${label} did not become ready within ${timeoutMs}ms` +
      (lastError ? `: ${lastError instanceof Error ? lastError.message : String(lastError)}` : ""),
  );
}
