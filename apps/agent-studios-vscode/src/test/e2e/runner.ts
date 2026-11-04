import * as fs from 'fs';
import * as path from 'path';
import Mocha = require('mocha');
import { vscodeMock } from '../unit/vscodeMock';

// Intercept 'vscode' import in Node environment for standalone e2e tests
// eslint-disable-next-line @typescript-eslint/no-var-requires
const nodeModule = require('module');
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const originalRequire = (nodeModule.prototype as any).require;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(nodeModule.prototype as any).require = function (modulePath: string, ...args: unknown[]) {
  if (modulePath === 'vscode') {
    return vscodeMock;
  }
  return originalRequire.apply(this, [modulePath, ...args]);
};

async function run(): Promise<void> {
  const mocha = new Mocha({
    ui: 'bdd',
    color: true,
    timeout: 20000,
  });

  const testDir = __dirname;
  const files = fs.readdirSync(testDir);
  for (const file of files) {
    if (file.endsWith('.test.js')) {
      mocha.addFile(path.join(testDir, file));
    }
  }

  return new Promise<void>((resolve, reject) => {
    mocha.run((failures: number) => {
      if (failures > 0) {
        reject(new Error(`${failures} e2e tests failed.`));
      } else {
        resolve();
      }
    });
  });
}

run().catch((err: unknown) => {
  console.error(err);
  process.exit(1);
});
