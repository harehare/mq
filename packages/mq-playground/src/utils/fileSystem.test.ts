import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { OPFSFileSystem } from "./fileSystem";

class MockFileHandle {
  content = "";

  async createWritable() {
    return {
      write: async (c: string) => {
        this.content = c;
      },
      close: async () => {},
    };
  }

  async getFile() {
    return { text: async () => this.content };
  }
}

class MockDirectoryHandle {
  private _files = new Map<string, MockFileHandle>();
  private _dirs = new Map<string, MockDirectoryHandle>();

  async getFileHandle(name: string, opts?: { create?: boolean }) {
    if (!this._files.has(name)) {
      if (!opts?.create) {
        throw Object.assign(new Error(`File not found: ${name}`), {
          name: "NotFoundError",
        });
      }
      this._files.set(name, new MockFileHandle());
    }
    return this._files.get(name)!;
  }

  async getDirectoryHandle(name: string, opts?: { create?: boolean }) {
    if (!this._dirs.has(name)) {
      if (!opts?.create) {
        throw Object.assign(new Error(`Directory not found: ${name}`), {
          name: "NotFoundError",
        });
      }
      this._dirs.set(name, new MockDirectoryHandle());
    }
    return this._dirs.get(name)!;
  }

  async removeEntry(name: string, _opts?: { recursive?: boolean }) {
    if (!this._files.delete(name) && !this._dirs.delete(name)) {
      throw Object.assign(new Error(`Entry not found: ${name}`), {
        name: "NotFoundError",
      });
    }
  }

  async *values() {
    for (const [name] of this._files) {
      yield { name, kind: "file" };
    }
    for (const [name] of this._dirs) {
      yield { name, kind: "directory" };
    }
  }
}

function setupMockStorage() {
  const root = new MockDirectoryHandle();
  vi.stubGlobal("navigator", {
    storage: { getDirectory: async () => root },
  });
  return root;
}

describe("OPFSFileSystem.isSupported", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns false when navigator.storage is absent", () => {
    vi.stubGlobal("navigator", {});
    expect(OPFSFileSystem.isSupported()).toBe(false);
  });

  it("returns false when getDirectory is absent", () => {
    vi.stubGlobal("navigator", { storage: {} });
    expect(OPFSFileSystem.isSupported()).toBe(false);
  });

  it("returns true when the full OPFS API is present", () => {
    vi.stubGlobal("navigator", {
      storage: { getDirectory: async () => {} },
    });
    expect(OPFSFileSystem.isSupported()).toBe(true);
  });
});

describe("OPFSFileSystem (uninitialized)", () => {
  it("writeFile throws before initialize", async () => {
    const fs = new OPFSFileSystem();
    await expect(fs.writeFile("/test.md", "x")).rejects.toThrow(
      "File system not initialized",
    );
  });

  it("readFile throws before initialize", async () => {
    const fs = new OPFSFileSystem();
    await expect(fs.readFile("/test.md")).rejects.toThrow(
      "File system not initialized",
    );
  });

  it("deleteFile throws before initialize", async () => {
    const fs = new OPFSFileSystem();
    await expect(fs.deleteFile("/test.md")).rejects.toThrow(
      "File system not initialized",
    );
  });

  it("listFiles throws before initialize", async () => {
    const fs = new OPFSFileSystem();
    await expect(fs.listFiles()).rejects.toThrow("File system not initialized");
  });

  it("createDirectory throws before initialize", async () => {
    const fs = new OPFSFileSystem();
    await expect(fs.createDirectory("/docs")).rejects.toThrow(
      "File system not initialized",
    );
  });
});

describe("OPFSFileSystem (initialized)", () => {
  let fs: OPFSFileSystem;

  beforeEach(async () => {
    setupMockStorage();
    fs = new OPFSFileSystem();
    await fs.initialize();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  describe("writeFile / readFile", () => {
    it("round-trips a simple file", async () => {
      await fs.writeFile("/hello.md", "# Hello");
      expect(await fs.readFile("/hello.md")).toBe("# Hello");
    });

    it("overwrites existing content", async () => {
      await fs.writeFile("/hello.md", "first");
      await fs.writeFile("/hello.md", "second");
      expect(await fs.readFile("/hello.md")).toBe("second");
    });

    it("creates intermediate directories automatically", async () => {
      await fs.writeFile("/a/b/c.md", "nested");
      expect(await fs.readFile("/a/b/c.md")).toBe("nested");
    });

    it("throws for empty path", async () => {
      await expect(fs.writeFile("", "x")).rejects.toThrow("Invalid file path");
    });

    it("multiple files coexist independently", async () => {
      await fs.writeFile("/foo.md", "foo");
      await fs.writeFile("/bar.md", "bar");
      expect(await fs.readFile("/foo.md")).toBe("foo");
      expect(await fs.readFile("/bar.md")).toBe("bar");
    });
  });

  describe("fileExists", () => {
    it("returns true for a written file", async () => {
      await fs.writeFile("/exists.md", "x");
      expect(await fs.fileExists("/exists.md")).toBe(true);
    });

    it("returns false for a path never written", async () => {
      expect(await fs.fileExists("/ghost.md")).toBe(false);
    });

    it("returns false after the file is deleted", async () => {
      await fs.writeFile("/del.md", "bye");
      await fs.deleteFile("/del.md");
      expect(await fs.fileExists("/del.md")).toBe(false);
    });
  });

  describe("createDirectory", () => {
    it("allows files to be written inside the created directory", async () => {
      await fs.createDirectory("/docs");
      await fs.writeFile("/docs/readme.md", "hi");
      expect(await fs.readFile("/docs/readme.md")).toBe("hi");
    });

    it("throws for an empty path", async () => {
      await expect(fs.createDirectory("")).rejects.toThrow(
        "Invalid directory path",
      );
    });

    it("throws when a file with the same name already exists", async () => {
      await fs.writeFile("/conflict.md", "");
      await expect(fs.createDirectory("/conflict.md")).rejects.toThrow(
        "already exists",
      );
    });
  });

  describe("listFiles", () => {
    it("returns an empty array for an empty root", async () => {
      expect(await fs.listFiles("/")).toEqual([]);
    });

    it("lists files in root with correct names and paths", async () => {
      await fs.writeFile("/a.md", "");
      await fs.writeFile("/b.md", "");
      const files = await fs.listFiles("/");
      expect(files.map((f) => f.name)).toEqual(["a.md", "b.md"]);
      expect(files.map((f) => f.path)).toEqual(["/a.md", "/b.md"]);
    });

    it("sorts directories before files", async () => {
      await fs.writeFile("/z.md", "");
      await fs.createDirectory("/a-folder");
      const files = await fs.listFiles("/");
      expect(files[0].type).toBe("directory");
      expect(files[1].type).toBe("file");
    });

    it("returns directory nodes with correct type and path", async () => {
      await fs.createDirectory("/src");
      const files = await fs.listFiles("/");
      expect(files).toHaveLength(1);
      expect(files[0]).toMatchObject({
        name: "src",
        type: "directory",
        path: "/src",
      });
    });
  });

  describe("renameFile", () => {
    it("moves content to the new path and removes the old path", async () => {
      await fs.writeFile("/old.md", "content");
      await fs.renameFile("/old.md", "/new.md");
      expect(await fs.readFile("/new.md")).toBe("content");
      expect(await fs.fileExists("/old.md")).toBe(false);
    });

    it("writes explicit content when provided", async () => {
      await fs.writeFile("/draft.md", "old");
      await fs.renameFile("/draft.md", "/final.md", "new content");
      expect(await fs.readFile("/final.md")).toBe("new content");
      expect(await fs.fileExists("/draft.md")).toBe(false);
    });
  });

  describe("isDirectoryPath", () => {
    it("returns true for an existing directory", async () => {
      await fs.createDirectory("/mydir");
      expect(await fs.isDirectoryPath("/mydir")).toBe(true);
    });

    it("returns false for an existing file", async () => {
      await fs.writeFile("/myfile.md", "");
      expect(await fs.isDirectoryPath("/myfile.md")).toBe(false);
    });

    it("returns false for a non-existent path", async () => {
      expect(await fs.isDirectoryPath("/nope")).toBe(false);
    });

    it("returns false for an empty path", async () => {
      expect(await fs.isDirectoryPath("")).toBe(false);
    });
  });
});
