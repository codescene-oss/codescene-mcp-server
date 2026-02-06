#!/usr/bin/env python3
"""
Test fixtures providing sample code for integration tests.

These fixtures include various code samples with different Code Health
characteristics to test the MCP server's analysis capabilities.
"""

# Simple, high-quality Python code (should score 9.0+)
GOOD_PYTHON_CODE = '''"""
Simple utility module with high code health.
"""

def calculate_total(items: list[float]) -> float:
    """Calculate the sum of all items."""
    return sum(items)


def calculate_average(items: list[float]) -> float:
    """Calculate the average of all items."""
    if not items:
        return 0.0
    return sum(items) / len(items)


def format_currency(amount: float) -> str:
    """Format an amount as currency."""
    return f"${amount:.2f}"
'''

# Complex Python code with issues (should score lower)
COMPLEX_PYTHON_CODE = '''"""
Complex module with maintainability issues.
"""

def process_order(order_data, customer_info, inventory, pricing_rules, discount_codes, shipping_options, payment_method):
    """Process a customer order with all business logic."""
    if not order_data or not customer_info:
        return {"error": "Missing data"}
    
    items = order_data.get("items", [])
    if not items:
        return {"error": "No items"}
    
    total = 0
    valid_items = []
    
    for item in items:
        if item["id"] not in inventory:
            continue
        
        stock = inventory[item["id"]]
        if stock["quantity"] < item["quantity"]:
            continue
        
        price = stock["price"]
        
        # Apply pricing rules
        for rule in pricing_rules:
            if rule["type"] == "bulk":
                if item["quantity"] >= rule["min_quantity"]:
                    price = price * (1 - rule["discount"])
            elif rule["type"] == "category":
                if stock["category"] == rule["category"]:
                    price = price * (1 - rule["discount"])
        
        # Apply discount codes
        for code in discount_codes:
            if code["code"] == order_data.get("discount_code"):
                if code["type"] == "percentage":
                    price = price * (1 - code["value"])
                elif code["type"] == "fixed":
                    price = max(0, price - code["value"])
        
        item_total = price * item["quantity"]
        total += item_total
        valid_items.append({"item": item, "price": price, "total": item_total})
    
    # Calculate shipping
    shipping_cost = 0
    if order_data.get("shipping_method") in shipping_options:
        shipping_option = shipping_options[order_data["shipping_method"]]
        if total < shipping_option["free_threshold"]:
            shipping_cost = shipping_option["cost"]
    
    # Calculate tax
    tax_rate = customer_info.get("tax_rate", 0.1)
    tax = total * tax_rate
    
    # Process payment
    final_total = total + shipping_cost + tax
    
    if payment_method == "credit_card":
        # Validate credit card
        if not customer_info.get("credit_card"):
            return {"error": "No credit card"}
        if not validate_credit_card(customer_info["credit_card"]):
            return {"error": "Invalid credit card"}
    elif payment_method == "paypal":
        if not customer_info.get("paypal_email"):
            return {"error": "No PayPal email"}
    
    return {
        "items": valid_items,
        "subtotal": total,
        "shipping": shipping_cost,
        "tax": tax,
        "total": final_total
    }


def validate_credit_card(card_info):
    """Validate credit card information."""
    # Simplified validation
    return len(card_info.get("number", "")) == 16
'''

# JavaScript code sample
JAVASCRIPT_CODE = """/**
 * User authentication service
 */

class AuthService {
  constructor(config) {
    this.config = config;
    this.users = new Map();
  }

  async login(username, password) {
    const user = this.users.get(username);
    
    if (!user) {
      throw new Error('User not found');
    }
    
    const isValid = await this.validatePassword(password, user.passwordHash);
    
    if (!isValid) {
      throw new Error('Invalid password');
    }
    
    return this.generateToken(user);
  }

  async register(username, password, email) {
    if (this.users.has(username)) {
      throw new Error('Username already exists');
    }
    
    const passwordHash = await this.hashPassword(password);
    
    const user = {
      username,
      passwordHash,
      email,
      createdAt: new Date()
    };
    
    this.users.set(username, user);
    return user;
  }

  async validatePassword(password, hash) {
    // Simplified validation
    return true;
  }

  async hashPassword(password) {
    // Simplified hashing
    return 'hashed_' + password;
  }

  generateToken(user) {
    return {
      token: 'token_' + user.username,
      expiresAt: new Date(Date.now() + 3600000)
    };
  }
}

module.exports = AuthService;
"""

# Java code sample with nested complexity
JAVA_CODE = """package com.example.service;

import java.util.*;

/**
 * Order processing service with business logic.
 */
public class OrderProcessor {
    
    private final InventoryService inventoryService;
    private final PaymentService paymentService;
    private final ShippingService shippingService;
    
    public OrderProcessor(
        InventoryService inventoryService,
        PaymentService paymentService,
        ShippingService shippingService
    ) {
        this.inventoryService = inventoryService;
        this.paymentService = paymentService;
        this.shippingService = shippingService;
    }
    
    public OrderResult processOrder(Order order) {
        if (order == null || order.getItems().isEmpty()) {
            return OrderResult.failure("Invalid order");
        }
        
        List<OrderItem> validItems = new ArrayList<>();
        double total = 0.0;
        
        for (OrderItem item : order.getItems()) {
            if (!inventoryService.isAvailable(item.getProductId(), item.getQuantity())) {
                continue;
            }
            
            double price = inventoryService.getPrice(item.getProductId());
            double itemTotal = price * item.getQuantity();
            total += itemTotal;
            validItems.add(item);
        }
        
        if (validItems.isEmpty()) {
            return OrderResult.failure("No valid items");
        }
        
        double shippingCost = shippingService.calculateShipping(
            order.getShippingAddress(),
            total
        );
        
        double tax = total * 0.1;
        double finalTotal = total + shippingCost + tax;
        
        PaymentResult paymentResult = paymentService.processPayment(
            order.getPaymentMethod(),
            finalTotal
        );
        
        if (!paymentResult.isSuccessful()) {
            return OrderResult.failure("Payment failed");
        }
        
        return OrderResult.success(validItems, finalTotal);
    }
}
"""


def get_sample_files() -> dict[str, str]:
    """
    Get a dictionary of sample files for testing.

    Returns:
        Dictionary mapping file paths to content
    """
    return {
        "src/utils/calculator.py": GOOD_PYTHON_CODE,
        "src/services/order_processor.py": COMPLEX_PYTHON_CODE,
        "src/auth/AuthService.js": JAVASCRIPT_CODE,
        "src/main/java/com/example/OrderProcessor.java": JAVA_CODE,
    }


def get_expected_scores() -> dict[str, tuple[float, float]]:
    """
    Get expected Code Health score ranges for sample files.

    Returns:
        Dictionary mapping file paths to (min_score, max_score) tuples
    """
    return {
        "src/utils/calculator.py": (8.5, 10.0),  # High quality
        "src/services/order_processor.py": (7.0, 9.0),  # Medium complexity
        "src/auth/AuthService.js": (7.0, 10.0),  # Good quality
        "src/main/java/com/example/OrderProcessor.java": (9.0, 10.0),  # High quality
    }
