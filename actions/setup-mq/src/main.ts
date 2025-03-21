import * as core from '@actions/core';
import * as github from '@actions/github';
import * as tc from '@actions/tool-cache';

const TOOL_NAME = 'mq';
const OWNER = 'harehare';
const REPO = 'mq';

const PLATFORM_MAP = {
  darwin_x64: 'x86_64-apple-darwin',
  darwin_arm64: 'aarch64-apple-darwin',
  win32_x64: 'x86_64-pc-windows-msvc.exe',
  win32_arm64: 'aarch64-pc-windows-msvc.exe',
  linux_arm64: 'aarch64-unknown-linux-gnu',
  linux_x64: 'x86_64-unknown-linux-gnu'
} as const;

type Platform = keyof typeof PLATFORM_MAP;

type Release = {
  version?: string;
  url?: string;
};

export async function run(): Promise<void> {
  try {
    const { arch, platform } = process;

    if (platform !== 'linux' && platform !== 'win32' && platform !== 'darwin') {
      core.error(`Not supported platform ${platform}`);
      return;
    }

    if (arch !== 'x64' && arch !== 'arm64') {
      core.error(`Not supported platform ${platform}`);
      return;
    }

    const version: string = core.getInput('version');
    const token = core.getInput('github-token');
    const release = await getRelease(
      token,
      OWNER,
      REPO,
      `${platform}_${arch}`,
      version
    );

    if (!release.url || !release.version) {
      core.info(
        `Not Found ${TOOL_NAME} version ${version} for ${platform}-${arch}`
      );
      return;
    }

    let toolPath = tc.find(TOOL_NAME, version);

    if (!toolPath) {
      const downloadPath = await tc.downloadTool(release.url);
      toolPath = await tc.cacheDir(
        downloadPath,
        TOOL_NAME,
        release.version,
        arch
      );
    }

    core.addPath(toolPath);
    core.info(
      `Setting up ${TOOL_NAME} version ${version} for ${platform}-${arch}`
    );
  } catch (error) {
    if (error instanceof Error) {
      core.setFailed(error.message);
    } else {
      core.setFailed('Unknown error occurred');
    }
  }
}

async function getRelease(
  token: string,
  owner: string,
  repo: string,
  platform: Platform,
  version?: string
): Promise<Release> {
  const octokit = github.getOctokit(token);

  if (!version || version === 'latest') {
    core.info(
      `No specific version provided. Fetching latest release for ${REPO}`
    );

    const latestReleaseResponse = await octokit.rest.repos.getLatestRelease({
      owner: OWNER,
      repo: REPO
    });

    core.info(`Latest release is ${latestReleaseResponse.data.tag_name}`);

    return {
      version: latestReleaseResponse.data.tag_name,
      url: latestReleaseResponse.data.assets.find(
        (asset: { name: string; browser_download_url: string }) =>
          asset.name === `mq-${PLATFORM_MAP[platform]}`
      )?.browser_download_url
    };
  } else {
    const tagVersion = version.startsWith('v') ? version : `v${version}`;
    const versionWithoutV = version.startsWith('v')
      ? version.substring(1)
      : version;

    core.info(`Fetching release information for ${REPO} ${version}`);

    try {
      const releaseResponse = await octokit.rest.repos.getReleaseByTag({
        owner: OWNER,
        repo: REPO,
        tag: tagVersion
      });

      return {
        version: releaseResponse.data.tag_name,
        url: releaseResponse.data.assets.find(
          (asset: { name: string; browser_download_url: string }) =>
            asset.name === `mq-${PLATFORM_MAP[platform]}`
        )?.browser_download_url
      };
    } catch {
      try {
        const releaseResponse = await octokit.rest.repos.getReleaseByTag({
          owner,
          repo,
          tag: versionWithoutV
        });

        return {
          version: releaseResponse.data.tag_name,
          url: releaseResponse.data.assets.find(
            (asset: { name: string; browser_download_url: string }) =>
              asset.name === `mq-${PLATFORM_MAP[platform]}`
          )?.browser_download_url
        };
      } catch (error) {
        if (error instanceof Error) {
          core.setFailed(error.message);
        } else {
          core.setFailed('Unknown error occurred');
        }
      }
    }
  }

  return {
    version,
    url: undefined
  };
}
