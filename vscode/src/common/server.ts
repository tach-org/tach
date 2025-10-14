// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

import { Disposable, env, LogOutputChannel, workspace } from 'vscode';
import { State } from 'vscode-languageclient';
import {
    LanguageClient,
    LanguageClientOptions,
    RevealOutputChannelOn,
    ServerOptions,
} from 'vscode-languageclient/node';
import { BUNDLED_PYTHON_LIBS_DIR } from './constants';
import { traceError, traceInfo, traceVerbose } from './log/logging';
import { getExtensionSettings, getGlobalSettings, getWorkspaceSettings, ISettings } from './settings';
import { getLSClientTraceLevel, getProjectRoot } from './utilities';
import { isVirtualWorkspace } from './vscodeapi';
import { execFile } from 'child_process';
import { supportsCustomConfig, VersionInfo } from './version';

export type IInitOptions = { settings: ISettings[]; globalSettings: ISettings };

function executeCommand(file: string, args: string[] = []): Promise<string> {
    return new Promise((resolve, reject) => {
      execFile(file, args, (error, stdout, stderr) => {
        if (error) {
          reject(new Error(stderr || error.message));
        } else {
          resolve(stdout);
        }
      });
    });
}

async function getTachVersion(pythonExecutable: string): Promise<VersionInfo> {
    const stdout = await executeCommand(pythonExecutable, ["-m", "tach", "--version"]);
    const version = stdout.trim().split(" ")[1];
    const [major, minor, patch] = version.split(".").map((x) => parseInt(x, 10));
    return new VersionInfo(major, minor, patch);
  }

async function createServer(
    settings: ISettings,
    serverId: string,
    serverName: string,
    outputChannel: LogOutputChannel,
    initializationOptions: IInitOptions,
): Promise<LanguageClient> {
    const command = settings.interpreter[0];
    const cwd = settings.cwd;
    const newEnv = { ...process.env };

    newEnv.LS_IMPORT_STRATEGY = settings.importStrategy;

    if (settings.importStrategy === 'useBundled') {
        newEnv.PYTHONPATH = BUNDLED_PYTHON_LIBS_DIR;
    }


    const args = settings.interpreter.slice(1).concat(["-m", "tach", "server"]);

    if (settings.configuration) {
        const version = await getTachVersion(command);
        if (!supportsCustomConfig(version)) {
            traceError(`Server: Tach version ${version.toString()} does not support custom configuration files.`);
        } else {
            traceInfo(`Server: Using custom configuration file: ${settings.configuration}`);
            args.push("-c", settings.configuration);
        }
    }

    traceInfo(`Server run command: ${[command, ...args].join(' ')}`);

    const serverOptions: ServerOptions = {
        command,
        args,
        options: { cwd, env: newEnv },
    };

    // Options to control the language client
    const clientOptions: LanguageClientOptions = {
        // Register the server for python documents
        documentSelector: isVirtualWorkspace()
            ? [{ language: 'python' }]
            : [
                  { scheme: 'file', language: 'python' },
                  { scheme: 'untitled', language: 'python' },
              ],
        outputChannel: outputChannel,
        traceOutputChannel: outputChannel,
        revealOutputChannelOn: RevealOutputChannelOn.Never,
        initializationOptions,
    };

    return new LanguageClient(serverId, serverName, serverOptions, clientOptions);
}

let _disposables: Disposable[] = [];

function createConfigWatcher(
    pattern: string,
    serverId: string,
    serverName: string,
    outputChannel: LogOutputChannel,
    lsClient: LanguageClient
): Disposable {
    return workspace.createFileSystemWatcher(pattern).onDidChange(async () => {
        traceInfo(`Configuration file changed, restarting server...`);
        await restartServer(serverId, serverName, outputChannel, lsClient);
    });
}

export async function restartServer(
    serverId: string,
    serverName: string,
    outputChannel: LogOutputChannel,
    lsClient?: LanguageClient,
): Promise<LanguageClient | undefined> {
    if (lsClient) {
        traceInfo(`Server: Stop requested`);
        await lsClient.stop();
        _disposables.forEach((d) => d.dispose());
        _disposables = [];
    }
    const projectRoot = await getProjectRoot();
    const workspaceSetting = await getWorkspaceSettings(serverId, projectRoot, true);

    const newLSClient = await createServer(workspaceSetting, serverId, serverName, outputChannel, {
        settings: await getExtensionSettings(serverId, true),
        globalSettings: await getGlobalSettings(serverId, false),
    });
    traceInfo(`Server: Start requested.`);
    _disposables.push(
        newLSClient.onDidChangeState((e) => {
            switch (e.newState) {
                case State.Stopped:
                    traceVerbose(`Server State: Stopped`);
                    break;
                case State.Starting:
                    traceVerbose(`Server State: Starting`);
                    break;
                case State.Running:
                    traceVerbose(`Server State: Running`);
                    break;
            }
        }),
    );
    try {
        await newLSClient.start();
        _disposables.push(
            createConfigWatcher(
                '**/{tach.toml,tach.domain.toml,pyproject.toml,requirements.txt}',
                serverId,
                serverName,
                outputChannel,
                newLSClient
            )
        );
    } catch (ex) {
        traceError(`Server: Start failed: ${ex}`);
        return undefined;
    }

    const level = getLSClientTraceLevel(outputChannel.logLevel, env.logLevel);
    await newLSClient.setTrace(level);
    return newLSClient;
}
