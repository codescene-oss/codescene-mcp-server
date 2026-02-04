#!/usr/bin/env python3
"""
Response parsing utilities for MCP tool responses.

This module provides functions for extracting data from MCP tool responses,
including Code Health scores and other structured content.
"""

import re
from typing import Optional


def extract_result_text(tool_response: dict) -> str:
    """
    Extract the actual result text from MCP response format.
    
    Args:
        tool_response: The tool response dictionary
        
    Returns:
        Extracted text content
    """
    if "result" not in tool_response:
        return ""
    result = tool_response["result"]
    if not isinstance(result, dict):
        return ""
    content = result.get("content", [])
    has_valid_content = content and isinstance(content, list) and len(content) > 0
    if has_valid_content:
        return content[0].get("text", "")
    structured = result.get("structuredContent", {})
    return structured.get("result", "")


def extract_code_health_score(response_text: str) -> Optional[float]:
    """
    Extract Code Health score from response text.
    
    Args:
        response_text: Response text from code_health_score or code_health_review tool
        
    Returns:
        The score as a float, or None if not found
    """
    # Try different patterns
    patterns = [
        r'code health score[:\s]+([0-9]+\.?[0-9]*)',
        r'score[:\s]+([0-9]+\.?[0-9]*)',
        r'health[:\s]+([0-9]+\.?[0-9]*)',
    ]
    
    text_lower = response_text.lower()
    for pattern in patterns:
        match = re.search(pattern, text_lower)
        if match:
            try:
                return float(match.group(1))
            except ValueError:
                continue
    
    return None
