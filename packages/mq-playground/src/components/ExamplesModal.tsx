import { useState } from "react";
import { VscClose } from "react-icons/vsc";
import { EXAMPLE_CATEGORIES } from "../examples";
import "./ExamplesModal.css";

type ExamplesModalProps = {
  isOpen: boolean;
  onClose: () => void;
  onSelect: (globalIndex: number) => void;
};

export const ExamplesModal = ({
  isOpen,
  onClose,
  onSelect,
}: ExamplesModalProps) => {
  const [activeCategoryIndex, setActiveCategoryIndex] = useState(0);

  if (!isOpen) return null;

  const activeCategory = EXAMPLE_CATEGORIES[activeCategoryIndex];
  const globalIndexOffset = EXAMPLE_CATEGORIES.slice(
    0,
    activeCategoryIndex,
  ).reduce((acc, cat) => acc + cat.examples.length, 0);

  return (
    <div className="examples-overlay" onClick={onClose}>
      <div
        className="examples-dialog"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="examples-header">
          <h3>Examples</h3>
          <button className="examples-close-btn" onClick={onClose}>
            <VscClose size={20} />
          </button>
        </div>
        <div className="examples-body">
          <div className="examples-sidebar">
            {EXAMPLE_CATEGORIES.map((category, idx) => (
              <button
                key={idx}
                className={`examples-category-btn ${activeCategoryIndex === idx ? "active" : ""}`}
                onClick={() => setActiveCategoryIndex(idx)}
              >
                {category.name}
              </button>
            ))}
          </div>
          <div className="examples-list">
            {activeCategory.examples.map((example, idx) => (
              <div
                key={idx}
                className="example-card"
                onClick={() => {
                  onSelect(globalIndexOffset + idx);
                  onClose();
                }}
              >
                <div className="example-card-name">{example.name}</div>
                <pre className="example-card-code">{example.code}</pre>
                {example.format && (
                  <div className="example-card-format">
                    <span>{example.format}</span>
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
};
