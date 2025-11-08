import {
  VscFile,
  VscFileCode,
  VscMarkdown,
  VscJson,
  VscFolder,
  VscFolderOpened,
} from "react-icons/vsc";

interface FileIconProps {
  fileName: string;
  isDirectory: boolean;
  isExpanded?: boolean;
}

export const FileIcon = ({
  fileName,
  isDirectory,
  isExpanded = false,
}: FileIconProps) => {
  if (isDirectory) {
    return isExpanded ? (
      <VscFolderOpened style={{ color: "#dcb67a" }} />
    ) : (
      <VscFolder style={{ color: "#dcb67a" }} />
    );
  }

  const extension = fileName.split(".").pop()?.toLowerCase();

  switch (extension) {
    case "mq":
      return <VscFileCode style={{ color: "#67b8e3" }} />;
    case "md":
    case "mdx":
      return <VscMarkdown style={{ color: "#519aba" }} />;
    case "json":
      return <VscJson style={{ color: "#cbcb41" }} />;
    default:
      return <VscFile />;
  }
};
