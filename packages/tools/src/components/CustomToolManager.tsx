import React, { useState, useEffect } from "react";
import { customToolsDB, type CustomTool } from "../db";
import { CustomToolForm } from "./CustomToolForm";

interface CustomToolManagerProps {
  onClose: () => void;
  onToolsChanged: () => void;
}

export const CustomToolManager: React.FC<CustomToolManagerProps> = ({
  onClose,
  onToolsChanged,
}) => {
  const [customTools, setCustomTools] = useState<CustomTool[]>([]);
  const [editingTool, setEditingTool] = useState<CustomTool | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadCustomTools();
  }, []);

  const loadCustomTools = async () => {
    try {
      setIsLoading(true);
      setError(null);
      const tools = await customToolsDB.getAllTools();
      setCustomTools(tools.sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime()));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load custom tools");
    } finally {
      setIsLoading(false);
    }
  };

  const handleDeleteTool = async (id: string) => {
    if (!confirm("Are you sure you want to delete this custom tool?")) {
      return;
    }

    try {
      await customToolsDB.deleteTool(id);
      await loadCustomTools();
      onToolsChanged();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete tool");
    }
  };

  const handleEditTool = (tool: CustomTool) => {
    setEditingTool(tool);
    setShowForm(true);
  };

  const handleFormSuccess = async () => {
    setShowForm(false);
    setEditingTool(null);
    await loadCustomTools();
    onToolsChanged();
  };

  const handleFormCancel = () => {
    setShowForm(false);
    setEditingTool(null);
  };

  if (showForm) {
    return (
      <div className="custom-tool-manager">
        <div className="manager-header">
          <h2>{editingTool ? "Edit Custom Tool" : "Add Custom Tool"}</h2>
        </div>
        <CustomToolForm
          tool={editingTool}
          onSuccess={handleFormSuccess}
          onCancel={handleFormCancel}
        />
      </div>
    );
  }

  return (
    <div className="custom-tool-manager">
      <div className="manager-header">
        <h2>Custom Tools Manager</h2>
        <div className="manager-actions">
          <button
            onClick={() => setShowForm(true)}
            className="btn btn-primary"
          >
            ➕ Add Tool
          </button>
          <button onClick={onClose} className="btn btn-secondary">
            ✕ Close
          </button>
        </div>
      </div>

      {error && (
        <div className="error-message">
          <p>⚠️ {error}</p>
        </div>
      )}

      <div className="tools-list">
        {isLoading ? (
          <div className="loading">Loading custom tools...</div>
        ) : customTools.length === 0 ? (
          <div className="empty-state">
            <p>No custom tools created yet.</p>
            <p>Click "Add Tool" to create your first custom tool.</p>
          </div>
        ) : (
          customTools.map((tool) => (
            <div key={tool.id} className="tool-item">
              <div className="tool-info">
                <h3 className="tool-name">{tool.name}</h3>
                <p className="tool-description">{tool.description}</p>
                <div className="tool-meta">
                  <code className="tool-query">{tool.query}</code>
                  <span className="tool-dates">
                    Created: {tool.createdAt.toLocaleDateString()}
                    {tool.updatedAt.getTime() !== tool.createdAt.getTime() && (
                      <> • Updated: {tool.updatedAt.toLocaleDateString()}</>
                    )}
                  </span>
                </div>
              </div>
              <div className="tool-actions">
                <button
                  onClick={() => handleEditTool(tool)}
                  className="btn btn-small btn-secondary"
                  title="Edit tool"
                >
                  ✏️ Edit
                </button>
                <button
                  onClick={() => handleDeleteTool(tool.id)}
                  className="btn btn-small btn-danger"
                  title="Delete tool"
                >
                  🗑️ Delete
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};