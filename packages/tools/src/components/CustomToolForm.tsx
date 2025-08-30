import React, { useState } from "react";
import { customToolsDB, type CustomTool } from "../db";

interface CustomToolFormProps {
  tool?: CustomTool | null;
  onSuccess: () => void;
  onCancel: () => void;
}

export const CustomToolForm: React.FC<CustomToolFormProps> = ({
  tool,
  onSuccess,
  onCancel,
}) => {
  const [formData, setFormData] = useState({
    name: tool?.name || "",
    description: tool?.description || "",
    query: tool?.query || "",
  });
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleInputChange = (
    e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>
  ) => {
    const { name, value } = e.target;
    setFormData((prev) => ({ ...prev, [name]: value }));
  };

  const validateForm = (): boolean => {
    if (!formData.name.trim()) {
      setError("Tool name is required");
      return false;
    }
    if (!formData.description.trim()) {
      setError("Tool description is required");
      return false;
    }
    if (!formData.query.trim()) {
      setError("Tool query is required");
      return false;
    }
    return true;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!validateForm()) {
      return;
    }

    setIsSubmitting(true);
    setError(null);

    try {
      if (tool) {
        // Update existing tool
        await customToolsDB.updateTool(tool.id, {
          name: formData.name.trim(),
          description: formData.description.trim(),
          query: formData.query.trim(),
        });
      } else {
        // Create new tool
        await customToolsDB.addTool({
          name: formData.name.trim(),
          description: formData.description.trim(),
          query: formData.query.trim(),
        });
      }
      onSuccess();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save tool");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="custom-tool-form">
      {error && (
        <div className="error-message">
          <p>⚠️ {error}</p>
        </div>
      )}

      <div className="form-group">
        <label htmlFor="name">Tool Name *</label>
        <input
          type="text"
          id="name"
          name="name"
          value={formData.name}
          onChange={handleInputChange}
          placeholder="Enter a descriptive name for your tool"
          required
        />
      </div>

      <div className="form-group">
        <label htmlFor="description">Description *</label>
        <input
          type="text"
          id="description"
          name="description"
          value={formData.description}
          onChange={handleInputChange}
          placeholder="Describe what this tool does"
          required
        />
      </div>

      <div className="form-group">
        <label htmlFor="query">mq Query *</label>
        <textarea
          id="query"
          name="query"
          value={formData.query}
          onChange={handleInputChange}
          placeholder="Enter the mq query (e.g., '.h | to_text()')"
          rows={4}
          required
        />
        <p className="form-help">
          Enter an mq query that will be applied to the input markdown.
          <br />
          Examples: <code>.h | to_text()</code>, <code>.link.url</code>,{" "}
          <code>.code</code>
          <br />
          📖 Need help? Check out the{" "}
          <a
            href="https://mqlang.org/book/"
            target="_blank"
            rel="noopener noreferrer"
            className="help-link"
          >
            mq documentation
          </a>
        </p>
      </div>

      <div className="form-actions">
        <button
          type="submit"
          disabled={isSubmitting}
          className="btn btn-primary"
        >
          {isSubmitting
            ? tool
              ? "Updating..."
              : "Creating..."
            : tool
            ? "Update Tool"
            : "Create Tool"}
        </button>
        <button
          type="button"
          onClick={onCancel}
          disabled={isSubmitting}
          className="btn btn-secondary"
        >
          Cancel
        </button>
      </div>
    </form>
  );
};
