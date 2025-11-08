export interface FileNode {
  name: string;
  path: string;
  type: "file" | "directory";
  children?: FileNode[];
}

export class OPFSFileSystem {
  private root: FileSystemDirectoryHandle | null = null;

  static isSupported(): boolean {
    return "storage" in navigator && "getDirectory" in navigator.storage;
  }

  async initialize(): Promise<void> {
    if (!OPFSFileSystem.isSupported()) {
      throw new Error(
        "Origin Private File System is not supported in this browser"
      );
    }
    this.root = await navigator.storage.getDirectory();
  }

  async writeFile(path: string, content: string): Promise<void> {
    if (!this.root) throw new Error("File system not initialized");

    const parts = path.split("/").filter(Boolean);
    const fileName = parts.pop();
    if (!fileName) throw new Error("Invalid file path");

    let currentDir = this.root;
    for (const part of parts) {
      currentDir = await currentDir.getDirectoryHandle(part, { create: true });
    }

    const fileHandle = await currentDir.getFileHandle(fileName, {
      create: true,
    });
    const writable = await fileHandle.createWritable();
    await writable.write(content);
    await writable.close();
  }

  async readFile(path: string): Promise<string> {
    if (!this.root) throw new Error("File system not initialized");

    const parts = path.split("/").filter(Boolean);
    const fileName = parts.pop();
    if (!fileName) throw new Error("Invalid file path");

    let currentDir = this.root;
    for (const part of parts) {
      currentDir = await currentDir.getDirectoryHandle(part);
    }

    const fileHandle = await currentDir.getFileHandle(fileName);
    const file = await fileHandle.getFile();
    return await file.text();
  }

  async deleteFile(path: string): Promise<void> {
    if (!this.root) throw new Error("File system not initialized");

    const parts = path.split("/").filter(Boolean);
    const fileName = parts.pop();
    if (!fileName) throw new Error("Invalid file path");

    let currentDir = this.root;
    for (const part of parts) {
      currentDir = await currentDir.getDirectoryHandle(part);
    }

    try {
      await currentDir.removeEntry(fileName);
    } catch (error) {
      console.error(`Failed to delete file ${path}:`, error);

      // Check if file actually exists before retrying
      try {
        await currentDir.getFileHandle(fileName);
        // File exists, retry deletion after delay
        await new Promise((resolve) => setTimeout(resolve, 500));
        await currentDir.removeEntry(fileName);
      } catch (checkError) {
        // File doesn't exist, which is fine (already deleted)
        console.log(
          `File ${path} doesn't exist, considering deletion successful`
        );
      }
    }
  }

  async createDirectory(path: string): Promise<void> {
    if (!this.root) throw new Error("File system not initialized");

    const parts = path.split("/").filter(Boolean);
    if (parts.length === 0) throw new Error("Invalid directory path");

    const dirName = parts[parts.length - 1];
    const parentParts = parts.slice(0, -1);

    try {
      // Navigate to parent directory
      let parentDir = this.root;
      for (const part of parentParts) {
        try {
          parentDir = await parentDir.getDirectoryHandle(part);
        } catch {
          parentDir = await parentDir.getDirectoryHandle(part, {
            create: true,
          });
        }
      }

      // Check if a file with this name exists
      try {
        await parentDir.getFileHandle(dirName);
        throw new Error(`A file named "${dirName}" already exists`);
      } catch (e) {
        // If error is "not found", that's good - no file exists
        // If error is our custom error, re-throw it
        if (e instanceof Error && e.message.includes("already exists")) {
          throw e;
        }
        // Otherwise, file doesn't exist, continue
      }

      // Create or get the directory
      await parentDir.getDirectoryHandle(dirName, { create: true });
    } catch (error) {
      console.error(`Failed to create directory ${path}:`, error);
      throw error;
    }
  }

  async deleteDirectory(path: string): Promise<void> {
    if (!this.root) throw new Error("File system not initialized");

    const parts = path.split("/").filter(Boolean);
    const dirName = parts.pop();
    if (!dirName) throw new Error("Invalid directory path");

    let currentDir = this.root;
    for (const part of parts) {
      currentDir = await currentDir.getDirectoryHandle(part);
    }

    await currentDir.removeEntry(dirName, { recursive: true });
  }

  async renameFile(
    oldPath: string,
    newPath: string,
    content?: string
  ): Promise<void> {
    if (!this.root) throw new Error("File system not initialized");

    try {
      if (content !== undefined) {
        // Content provided - write new file and delete old
        await this.writeFile(newPath, content);

        // Try to delete the old file
        try {
          await this.deleteFile(oldPath);
        } catch (deleteError) {
          // Check if new file exists
          const newExists = await this.fileExists(newPath);
          if (!newExists) {
            throw new Error("New file was not created successfully");
          }
          console.error(deleteError);
        }
      } else {
        // No content provided - read, write, delete
        const isDirectory = await this.isDirectory(oldPath);

        if (isDirectory) {
          await this.copyDirectory(oldPath, newPath);
          await this.deleteDirectory(oldPath);
        } else {
          const fileContent = await this.readFile(oldPath);
          await this.writeFile(newPath, fileContent);
          await this.deleteFile(oldPath);
        }
      }
    } catch (error) {
      // Check if the new file was created successfully despite the error
      try {
        const newExists = await this.fileExists(newPath);
        if (newExists) {
          return; // Success - new file exists
        }
      } catch {
        // Ignore check error
      }

      throw new Error(
        `Failed to rename from "${oldPath}" to "${newPath}": ${
          error instanceof Error ? error.message : String(error)
        }`
      );
    }
  }

  async isDirectoryPath(path: string): Promise<boolean> {
    if (!this.root) throw new Error("File system not initialized");

    try {
      const parts = path.split("/").filter(Boolean);
      const name = parts.pop();
      if (!name) return false;

      let currentDir = this.root;
      for (const part of parts) {
        currentDir = await currentDir.getDirectoryHandle(part);
      }

      // Try to get as directory
      await currentDir.getDirectoryHandle(name);
      return true;
    } catch {
      return false;
    }
  }

  private async isDirectory(path: string): Promise<boolean> {
    return this.isDirectoryPath(path);
  }

  private async copyDirectory(
    sourcePath: string,
    destPath: string
  ): Promise<void> {
    if (!this.root) throw new Error("File system not initialized");

    // Create destination directory
    await this.createDirectory(destPath);

    // List all files in source directory
    const files = await this.listFiles(sourcePath);

    // Copy each file/directory recursively
    for (const file of files) {
      const sourceFilePath = file.path;
      const relativePath = sourceFilePath.substring(sourcePath.length);
      const destFilePath = destPath + relativePath;

      if (file.type === "directory") {
        await this.copyDirectory(sourceFilePath, destFilePath);
      } else {
        const content = await this.readFile(sourceFilePath);
        await this.writeFile(destFilePath, content);
      }
    }
  }

  async listFiles(path: string = "/"): Promise<FileNode[]> {
    if (!this.root) throw new Error("File system not initialized");

    const parts = path.split("/").filter(Boolean);
    let currentDir = this.root;
    for (const part of parts) {
      currentDir = await currentDir.getDirectoryHandle(part);
    }

    const result: FileNode[] = [];
    // @ts-expect-error - TypeScript doesn't have the latest OPFS types
    for await (const entry of currentDir.values()) {
      const nodePath =
        path === "/" ? `/${entry.name}` : `${path}/${entry.name}`;
      if (entry.kind === "directory") {
        const children = await this.listFiles(nodePath);
        result.push({
          name: entry.name,
          path: nodePath,
          type: "directory",
          children: children.sort((a, b) => {
            // Directories first, then files
            if (a.type !== b.type) {
              return a.type === "directory" ? -1 : 1;
            }
            return a.name.localeCompare(b.name);
          }),
        });
      } else {
        result.push({
          name: entry.name,
          path: nodePath,
          type: "file",
        });
      }
    }

    // Sort: directories first, then files, both alphabetically
    return result.sort((a, b) => {
      if (a.type !== b.type) {
        return a.type === "directory" ? -1 : 1;
      }
      return a.name.localeCompare(b.name);
    });
  }

  async fileExists(path: string): Promise<boolean> {
    if (!this.root) return false;

    try {
      await this.readFile(path);
      return true;
    } catch {
      return false;
    }
  }
}

export const fileSystem = new OPFSFileSystem();
