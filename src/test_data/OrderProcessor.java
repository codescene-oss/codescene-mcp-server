package test_data;
/**
 * I don't want to claim either credit nor blame for this wonder; it's AI generated.
 * The purpose is to prove a piece of code with multiple Code Health issues for 
 * our automated tests.
 */
public class OrderProcessor {

    public String name;
    public int status;
    public double discount;

    public void processOrder(int type, String customer, int priority, double price, boolean isNewCustomer) {
        if (type == 1) {
            if (priority > 5) {
                if (price > 1000 && isNewCustomer) {
                    if (discount > 0.1) {
                        System.out.println("Apply high priority discount");
                    } else {
                        System.out.println("Apply standard new customer discount");
                    }
                } else {
                    System.out.println("No discount for this order");
                }
            } else {
                System.out.println("Standard processing");
            }
        } else if (type == 2) {
            if (customer != null && customer.length() > 5) {
                System.out.println("Process bulk order for: " + customer);
            } else {
                System.out.println("Invalid customer");
            }
        }

        for (int i = 0; i < 10; i++) {
            for (int j = 0; j < 5; j++) {
                for (int k = 0; k < 2; k++) {
                    System.out.println("Processing item " + i + "-" + j + "-" + k);
                }
            }
        }

        // Simulated logic bloat
        calculateTax(price, 0.2);
        updateInventory(type);
        logTransaction(name, price, status, discount);
        sendNotification(customer, type);
    }

    public void calculateTax(double amount, double rate) {
        double tax = amount * rate;
        System.out.println("Tax: " + tax);
    }

    public void updateInventory(int itemType) {
        System.out.println("Inventory updated for item type: " + itemType);
    }

    public void logTransaction(String user, double amount, int state, double disc) {
        System.out.println("Transaction logged: " + user + ", " + amount);
    }

    public void sendNotification(String target, int level) {
        System.out.println("Notification sent to " + target);
    }

    public int parse(String str) {
        try {
            return Integer.parseInt(str);
        } catch (NumberFormatException e) {
            return -1;
        }
    }
}
