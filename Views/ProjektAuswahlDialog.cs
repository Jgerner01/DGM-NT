using System;
using System.IO;
using System.Windows;
using System.Windows.Controls;

namespace DGM_NT.Views;

/// <summary>
/// Dialog zur Auswahl eines Projekts wenn mehrere im Verzeichnis sind
/// </summary>
public class ProjektAuswahlDialog : Window
{
    public string AusgewaehlteDatei { get; private set; } = "";
    
    public ProjektAuswahlDialog(string[] dgmntDateien, string verzeichnisPfad)
    {
        Title = "Projekt auswählen";
        Width = 600;
        Height = 400;
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        ResizeMode = ResizeMode.NoResize;
        Background = new System.Windows.Media.SolidColorBrush(System.Windows.Media.Color.FromRgb(245, 245, 245));
        
        var grid = new Grid { Margin = new Thickness(20) };
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        grid.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        
        // Titel
        var titelPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 10) };
        titelPanel.Children.Add(new TextBlock 
        { 
            Text = "Verzeichnis enthält mehrere Projekte",
            FontSize = 16,
            FontWeight = System.Windows.FontWeights.Bold
        });
        titelPanel.Children.Add(new TextBlock 
        { 
            Text = $"Verzeichnis: {verzeichnisPfad}",
            FontSize = 12,
            Foreground = System.Windows.Media.Brushes.Gray,
            Margin = new Thickness(0, 5, 0, 0)
        });
        Grid.SetRow(titelPanel, 0);
        grid.Children.Add(titelPanel);
        
        // Projektliste
        var listPanel = new StackPanel();
        listPanel.Children.Add(new TextBlock 
        { 
            Text = "Bitte wählen Sie ein Projekt:",
            FontWeight = System.Windows.FontWeights.SemiBold,
            Margin = new Thickness(0, 0, 0, 5)
        });
        
        var listBox = new ListBox 
        { 
            FontSize = 13,
            BorderThickness = new Thickness(1),
            BorderBrush = System.Windows.Media.Brushes.LightGray,
            Background = System.Windows.Media.Brushes.White
        };
        
        foreach (var datei in dgmntDateien)
        {
            string name = Path.GetFileNameWithoutExtension(datei);
            string info = "";
            
            try
            {
                // Versuche zusätzliche Info zu laden
                var json = File.ReadAllText(datei);
                // Einfach Dateiname und Größe anzeigen
                var fi = new FileInfo(datei);
                info = $"{name} ({fi.Length / 1024} KB, zuletzt: {fi.LastWriteTime:dd.MM.yyyy})";
            }
            catch
            {
                info = name;
            }
            
            listBox.Items.Add(new ListBoxItem { Content = info, Tag = datei });
        }
        
        listBox.SelectedIndex = 0;
        listBox.Height = 200;
        listPanel.Children.Add(listBox);
        
        Grid.SetRow(listPanel, 1);
        grid.Children.Add(listPanel);
        
        // Buttons
        var btnPanel = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right, Margin = new Thickness(0, 15, 0, 0) };
        
        var btnOk = new Button 
        { 
            Content = "Projekt laden", 
            Padding = new Thickness(20, 8, 20, 8),
            Margin = new Thickness(0, 0, 10, 0),
            IsDefault = true
        };
        btnOk.Click += (s, e) =>
        {
            if (listBox.SelectedItem is ListBoxItem item && item.Tag is string datei)
            {
                AusgewaehlteDatei = datei;
                DialogResult = true;
                Close();
            }
        };
        btnPanel.Children.Add(btnOk);
        
        var btnAbbruch = new Button { Content = "Abbruch", Padding = new Thickness(15, 8, 15, 8), IsCancel = true };
        btnAbbruch.Click += (s, e) => DialogResult = false;
        btnPanel.Children.Add(btnAbbruch);
        
        Grid.SetRow(btnPanel, 2);
        grid.Children.Add(btnPanel);
        
        Content = grid;
    }
}
