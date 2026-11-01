// Prints the licences of the npm packages that end up in the app's frontend
// bundle: the production dependencies in package-lock.json, read from
// node_modules. Deterministic: sorted by name, no dates or paths.
// Part of scripts/gen-licences.sh (#24); needs `npm ci` first.
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const lock = JSON.parse(readFileSync("package-lock.json", "utf8"));
const entries = Object.entries(lock.packages ?? {})
  // "" is the app itself; dev-only entries do not ship.
  .filter(([path, info]) => path.startsWith("node_modules/") && !info.dev)
  .map(([path]) => path.slice("node_modules/".length))
  .sort();

function licenceFiles(dir) {
  const names = readdirSync(dir).filter((name) => /^(licen[sc]e|copying|notice)/i.test(name));
  names.sort();
  return names.map((name) => readFileSync(join(dir, name), "utf8").trimEnd());
}

const out = [];
for (const name of entries) {
  const dir = join("node_modules", name);
  if (!existsSync(join(dir, "package.json"))) {
    // An optional dependency for another platform is not installed here.
    continue;
  }
  const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
  const licence = typeof pkg.license === "string" ? pkg.license : JSON.stringify(pkg.license);
  const repository = typeof pkg.repository === "string" ? pkg.repository : pkg.repository?.url;
  out.push("================================================================================");
  out.push(`${pkg.name} ${pkg.version} (${licence})`);
  if (repository) out.push(repository);
  out.push("");
  const texts = licenceFiles(dir);
  out.push(texts.length > 0 ? texts.join("\n\n") : "(no licence file in the package)");
  out.push("");
}
process.stdout.write(out.join("\n") + "\n");
