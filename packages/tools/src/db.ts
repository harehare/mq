export interface CustomTool {
  id: string;
  name: string;
  description: string;
  query: string;
  category: "Custom";
  createdAt: Date;
  updatedAt: Date;
}

class CustomToolsDB {
  private dbName = "CustomToolsDB";
  private version = 1;
  private storeName = "customTools";

  private async openDB(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open(this.dbName, this.version);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(request.result);

      request.onupgradeneeded = (event) => {
        const db = (event.target as IDBOpenDBRequest).result;
        
        if (!db.objectStoreNames.contains(this.storeName)) {
          const store = db.createObjectStore(this.storeName, { keyPath: "id" });
          store.createIndex("name", "name", { unique: false });
          store.createIndex("category", "category", { unique: false });
        }
      };
    });
  }

  async getAllTools(): Promise<CustomTool[]> {
    const db = await this.openDB();
    
    return new Promise((resolve, reject) => {
      const transaction = db.transaction([this.storeName], "readonly");
      const store = transaction.objectStore(this.storeName);
      const request = store.getAll();

      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const tools = request.result.map((tool: any) => ({
          ...tool,
          createdAt: new Date(tool.createdAt),
          updatedAt: new Date(tool.updatedAt),
        }));
        resolve(tools);
      };
    });
  }

  async getTool(id: string): Promise<CustomTool | null> {
    const db = await this.openDB();
    
    return new Promise((resolve, reject) => {
      const transaction = db.transaction([this.storeName], "readonly");
      const store = transaction.objectStore(this.storeName);
      const request = store.get(id);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const tool = request.result;
        if (tool) {
          resolve({
            ...tool,
            createdAt: new Date(tool.createdAt),
            updatedAt: new Date(tool.updatedAt),
          });
        } else {
          resolve(null);
        }
      };
    });
  }

  async addTool(toolData: Omit<CustomTool, "id" | "category" | "createdAt" | "updatedAt">): Promise<CustomTool> {
    const db = await this.openDB();
    const now = new Date();
    const tool: CustomTool = {
      ...toolData,
      id: this.generateId(),
      category: "Custom",
      createdAt: now,
      updatedAt: now,
    };

    return new Promise((resolve, reject) => {
      const transaction = db.transaction([this.storeName], "readwrite");
      const store = transaction.objectStore(this.storeName);
      const request = store.add(tool);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(tool);
    });
  }

  async updateTool(id: string, updates: Partial<Pick<CustomTool, "name" | "description" | "query">>): Promise<CustomTool> {
    const db = await this.openDB();
    const existingTool = await this.getTool(id);
    
    if (!existingTool) {
      throw new Error(`Tool with id ${id} not found`);
    }

    const updatedTool: CustomTool = {
      ...existingTool,
      ...updates,
      updatedAt: new Date(),
    };

    return new Promise((resolve, reject) => {
      const transaction = db.transaction([this.storeName], "readwrite");
      const store = transaction.objectStore(this.storeName);
      const request = store.put(updatedTool);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(updatedTool);
    });
  }

  async deleteTool(id: string): Promise<void> {
    const db = await this.openDB();

    return new Promise((resolve, reject) => {
      const transaction = db.transaction([this.storeName], "readwrite");
      const store = transaction.objectStore(this.storeName);
      const request = store.delete(id);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve();
    });
  }

  private generateId(): string {
    return `custom_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }
}

export const customToolsDB = new CustomToolsDB();