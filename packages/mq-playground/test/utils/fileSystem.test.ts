import { describe, it, expect, beforeEach } from "vitest";
import { OPFSFileSystem } from "../../src/utils/fileSystem";

// Minimal in-memory fake of the Origin Private File System API, just enough
// of FileSystemDirectoryHandle/FileSystemFileHandle for OPFSFileSystem to
// exercise its traversal, read/write, and error-recovery logic under jsdom
// (which has no real OPFS implementation).
class FakeFileHandle {
  kind = "file" as const;
  content = "";
  constructor(public name: string) {}

  async getFile() {
    const content = this.content;
    return { text: async () => content };
  }

  async createWritable() {
    let buffer = "";
    return {
      write: async (data: string) => {
        buffer = data;
      },
      close: async () => {
        this.content = buffer;
      },
    };
  }
}

class FakeDirectoryHandle {
  kind = "directory" as const;
  entries = new Map<string, FakeDirectoryHandle | FakeFileHandle>();
  constructor(public name: string) {}

  async getDirectoryHandle(
    name: string,
    options?: { create?: boolean },
  ): Promise<FakeDirectoryHandle> {
    const existing = this.entries.get(name);
    if (existing instanceof FakeDirectoryHandle) return existing;
    if (existing instanceof FakeFileHandle) {
      throw new Error(`${name} is a file`);
    }
    if (!options?.create) {
      throw new Error(`Directory not found: ${name}`);
    }
    const dir = new FakeDirectoryHandle(name);
    this.entries.set(name, dir);
    return dir;
  }

  async getFileHandle(
    name: string,
    options?: { create?: boolean },
  ): Promise<FakeFileHandle> {
    const existing = this.entries.get(name);
    if (existing instanceof FakeFileHandle) return existing;
    if (existing instanceof FakeDirectoryHandle) {
      throw new Error(`${name} is a directory`);
    }
    if (!options?.create) {
      throw new Error(`File not found: ${name}`);
    }
    const file = new FakeFileHandle(name);
    this.entries.set(name, file);
    return file;
  }

  async removeEntry(
    name: string,
    _options?: { recursive?: boolean },
  ): Promise<void> {
    if (!this.entries.has(name)) {
      throw new Error(`Entry not found: ${name}`);
    }
    this.entries.delete(name);
  }

  async *values() {
    for (const [name, handle] of this.entries) {
      yield { name, kind: handle.kind };
    }
  }
}

const installFakeOPFS = () => {
  const root = new FakeDirectoryHandle("");
  Object.defineProperty(globalThis.navigator, "storage", {
    configurable: true,
    value: { getDirectory: async () => root },
  });
  return root;
};

describe("OPFSFileSystem", () => {
  let fs: OPFSFileSystem;

  beforeEach(async () => {
    installFakeOPFS();
    fs = new OPFSFileSystem();
    await fs.initialize();
  });

  it("reports OPFS as supported when navigator.storage.getDirectory exists", () => {
    expect(OPFSFileSystem.isSupported()).toBe(true);
  });

  it("writes and reads back a top-level file", async () => {
    await fs.writeFile("/hello.mq", ".[]");
    expect(await fs.readFile("/hello.mq")).toBe(".[]");
  });

  it("creates intermediate directories when writing a nested file", async () => {
    await fs.writeFile("/a/b/c.mq", "content");
    expect(await fs.readFile("/a/b/c.mq")).toBe("content");
  });

  it("reports whether a file exists", async () => {
    expect(await fs.fileExists("/missing.mq")).toBe(false);
    await fs.writeFile("/present.mq", "x");
    expect(await fs.fileExists("/present.mq")).toBe(true);
  });

  it("deletes a file", async () => {
    await fs.writeFile("/temp.mq", "x");
    await fs.deleteFile("/temp.mq");
    expect(await fs.fileExists("/temp.mq")).toBe(false);
  });

  it("creates a directory explicitly", async () => {
    await fs.createDirectory("/dir");
    expect(await fs.isDirectoryPath("/dir")).toBe(true);
  });

  it("rejects creating a directory where a same-named file exists", async () => {
    await fs.writeFile("/conflict", "x");
    await expect(fs.createDirectory("/conflict")).rejects.toThrow(
      "already exists",
    );
  });

  it("lists files and directories sorted with directories first", async () => {
    await fs.writeFile("/z.mq", "z");
    await fs.writeFile("/a.mq", "a");
    await fs.createDirectory("/m");

    const listing = await fs.listFiles("/");
    expect(listing.map((n) => n.name)).toEqual(["m", "a.mq", "z.mq"]);
    expect(listing[0].type).toBe("directory");
  });

  it("renames a file by moving its content", async () => {
    await fs.writeFile("/old.mq", "payload");
    await fs.renameFile("/old.mq", "/new.mq", "payload");

    expect(await fs.fileExists("/old.mq")).toBe(false);
    expect(await fs.readFile("/new.mq")).toBe("payload");
  });

  it("deletes a directory recursively", async () => {
    await fs.writeFile("/dir/file.mq", "x");
    await fs.deleteDirectory("/dir");
    expect(await fs.isDirectoryPath("/dir")).toBe(false);
  });
});
