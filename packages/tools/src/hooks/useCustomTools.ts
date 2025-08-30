import { useState, useEffect, useCallback } from "react";
import { run } from "mq-web";
import { customToolsDB, type CustomTool } from "../db";
import type { Tool } from "../types";

const createTransformError = (error: unknown): Error => {
  if (error instanceof Error) {
    return new Error(
      error.message.includes("WebAssembly")
        ? "WebAssembly module failed to initialize. Please refresh the page and try again."
        : error.message
    );
  }
  return new Error("An unknown error occurred during transformation");
};

const safeRun = async (query: string, input: string): Promise<string> => {
  try {
    return await run(query, input, { inputFormat: "markdown" });
  } catch (error) {
    console.error("mq-web execution error:", error);
    throw createTransformError(error);
  }
};

export const useCustomTools = () => {
  const [customTools, setCustomTools] = useState<CustomTool[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadCustomTools = useCallback(async () => {
    try {
      setIsLoading(true);
      setError(null);
      const tools = await customToolsDB.getAllTools();
      setCustomTools(tools);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to load custom tools"
      );
      console.error("Failed to load custom tools:", err);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadCustomTools();
  }, [loadCustomTools]);

  const convertToTools = useCallback((customTools: CustomTool[]): Tool[] => {
    return customTools.map((customTool) => ({
      id: customTool.id,
      name: customTool.name,
      description: customTool.description,
      category: "Custom" as const,
      transform: async (input: string): Promise<string> => {
        return safeRun(customTool.query, input);
      },
    }));
  }, []);

  const toolsFromCustom = convertToTools(customTools);

  return {
    customTools,
    toolsFromCustom,
    isLoading,
    error,
    refreshCustomTools: loadCustomTools,
  };
};
