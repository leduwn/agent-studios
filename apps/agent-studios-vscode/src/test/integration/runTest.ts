import * as path from 'path';
import { runTests } from '@vscode/test-electron';

async function main(): Promise<void> {
  try {
    // When run from inside VS Code integrated terminals or Claude Code,
    // ELECTRON_RUN_AS_NODE is set. We must delete it so Code.exe launches as full Electron/VS Code.
    delete process.env.ELECTRON_RUN_AS_NODE;

    // The folder containing the Extension Manifest package.json
    const extensionDevelopmentPath = path.resolve(__dirname, '../../../');

    // The path to test runner script that exports `run()`
    const extensionTestsPath = path.resolve(__dirname, './index');

    console.log('[ExtensionHostTest] Launching real VS Code Extension Development Host...');
    console.log(`[ExtensionHostTest] Dev Path: ${extensionDevelopmentPath}`);
    console.log(`[ExtensionHostTest] Tests Path: ${extensionTestsPath}`);

    // Download VS Code, unzip it and run the integration test
    await runTests({
      extensionDevelopmentPath,
      extensionTestsPath,
      launchArgs: [
        '--disable-extensions',
        '--disable-gpu',
        '--disable-workspace-trust',
        '--skip-welcome',
        '--skip-release-notes',
      ],
    });

    console.log('[ExtensionHostTest] All real Extension Host tests passed successfully!');
  } catch (err) {
    console.error('[ExtensionHostTest] Failed to run tests in Extension Host:', err);
    process.exit(1);
  }
}

main();
