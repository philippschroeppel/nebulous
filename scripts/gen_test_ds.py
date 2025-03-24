from datasets import DatasetDict, load_dataset

# Load the full dataset
ds = load_dataset("trl-lib/Capybara")

# Select the first 100 records from each split (adjust as needed)
train_ds = ds["train"].select(range(100))
test_ds = ds["test"].select(range(100)) if "test" in ds else None

# Create a new DatasetDict that includes both splits
if test_ds is not None:
    ds_small = DatasetDict({"train": train_ds, "test": test_ds})
else:
    ds_small = DatasetDict({"train": train_ds})
    print("Warning: No test split found. Only the 'train' split will be available.")

# Push the slimmed dataset to the hub
ds_small.push_to_hub("agentsea/Capybara-slim")
